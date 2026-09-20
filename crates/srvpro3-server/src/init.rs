use super::server::Server;

use anyhow::{Error, anyhow};
use parking_lot::{Mutex, MutexGuard, RawRwLock, lock_api::RwLockReadGuard};
use std::{
	borrow::Cow,
	ffi::{CStr, c_char, c_int},
	fs::read,
	ptr::null_mut,
	sync::{Arc, LazyLock}
};
use tokio::{sync::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard, oneshot}, task::JoinHandle};
use ygopro::managers::{
	config_manager::{ConfigManager, set_global as set_config_manager},
	data_manager::{DataManager, card_reader, set_global as set_data_manager},
	deck_manager::{DeckManager, set_global as set_deck_manager},
};
use ygopro_core_wrapper::{intptr_t, set_card_reader, set_message_handler, set_script_reader};
use ygopro_data::{
	constants::{Attribute, Category, Linkmarkers, OT, Race, Type},
	data::{Card, CoreCard, LFList}
};

use srvpro3_cards::CardsSnapshot;
use srvpro3_config::Config;
use srvpro3_log::*;

static SCRIPT_BUFFER: Mutex<[u8; 0x100000]> = Mutex::new([0u8; 0x100000]);

struct RunningServer {
	tcp_port: u16,
	udp_port: u16,
	ws_port: u16,
	shutdown: oneshot::Sender<()>,
	task: JoinHandle<()>
}

static SERVER: LazyLock<AsyncMutex<Option<RunningServer>>> = LazyLock::new(|| AsyncMutex::new(None));

extern "C" fn script_reader(script_path: *const c_char, slen: *mut c_int) -> *mut u8 {
	fn read_file(file_path: &str, buffer: &mut [u8]) -> Result<usize, Error> {
		let data: Vec<u8> = read(file_path)?;
		if data.len() >= buffer.len() { return Err(anyhow!("脚本文件过大")); }
		buffer[..data.len()].copy_from_slice(&data);
		Ok(data.len())
	}
	if script_path.is_null() || slen.is_null() { return null_mut(); }
	let path: Cow<'_, str> = unsafe { CStr::from_ptr(script_path).to_string_lossy() };
	let mut buffer: MutexGuard<'_, [u8; 0x100000]> = SCRIPT_BUFFER.lock();
	let result: Result<usize, Error> = if path.starts_with("./script") {
		let script_name: &str = &path[2..];
		srvpro3_cards::script(script_name).and_then(|data| {
			if data.len() >= buffer.len() { return Err(anyhow!("脚本文件过大")); }
			buffer[..data.len()].copy_from_slice(&data);
			Ok(data.len())
		})
	} else {
		read_file(path.as_ref(), &mut *buffer)
	};
	match result {
		Ok(length) => {
			unsafe { *slen = length as c_int; }
			buffer.as_mut_ptr()
		}
		Err(_) => null_mut()
	}
}

extern "C" fn core_message_handler(_: intptr_t, _: u32) -> u32 {
	0
}

pub fn reload_cards() -> Result<(), Error> {
	let mut data_manager: DataManager = DataManager::new();
	let cards: Arc<CardsSnapshot> = srvpro3_cards::get()?;
	for card in cards.cards.values() {
		data_manager.cards.insert(
			card.code,
			Card {
				card: CoreCard {
					code: card.code,
					alias: card.alias,
					setcode: card.setcode,
					card_type: Type::from_bits_retain(card.card_type),
					level: card.level,
					attribute: Attribute::from_bits_retain(card.attribute),
					race: Race::from_bits_retain(card.race),
					attack: card.attack,
					defense: card.defense,
					left_scale: card.lscale,
					right_scale: card.rscale,
					link_marker: Linkmarkers::from_bits_retain(card.link_marker),
					rule_code: 0
				},
				ot: OT::from_bits_retain(card.ot),
				category: Category::from_bits_retain(card.category),
				name: String::new(),
				text: String::new(),
				desc: Default::default()
			}
		);
	}
	data_manager.finalize_db();
	let mut deck_manager = DeckManager::new();
	// 第 0 项保留给无禁限卡表，使 HostInfo.lflist = 0 下发的哈希也为 0。
	deck_manager.lflists.push(LFList {
		hash: 0,
		name: "无禁限".to_owned(),
		content: Default::default(),
		genesys: 0,
		glist: Default::default()
	});
	for (name, lflist) in &cards.lflists {
		deck_manager.lflists.push(LFList {
			hash: lflist.hash,
			name: name.to_string(),
			content: lflist.lflist.clone(),
			genesys: lflist.genesys,
			glist: lflist.glist.clone()
		});
	}
	set_data_manager(data_manager);
	set_deck_manager(deck_manager);
	Ok(())
}

pub async fn init() -> Result<(), Error> {
	set_config_manager(ConfigManager::new());
	unsafe {
		set_script_reader(Some(script_reader));
		set_card_reader(Some(card_reader));
		set_message_handler(Some(core_message_handler));
	}
	reload().await
}

pub async fn reload() -> Result<(), Error> {
	reload_cards()?;
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()?;
	let tcp_port: u16 = config.server.tcp.port;
	let udp_port: u16 = config.server.udp.port;
	let ws_port: u16 = config.server.ws.port;
	let reconnect_timeout: u64 = config.server.reconnect_timeout;
	let side_timeout: u64 = config.server.side_timeout;
	drop(config);

	let mut running: AsyncMutexGuard<'_, Option<RunningServer>> = SERVER.lock().await;
	if running.as_ref().is_some_and(|server| {
		server.tcp_port == tcp_port && server.udp_port == udp_port && server.ws_port == ws_port && !server.task.is_finished()
	}) {
		return Ok(());
	}
	if let Some(server) = running.take() {
		let _ = server.shutdown.send(());
		let _ = server.task.await;
	}
	let server: Server = Server::bind(tcp_port, udp_port, ws_port, reconnect_timeout, side_timeout).await?;
	let (shutdown, shutdown_rx) = oneshot::channel();
	let task: JoinHandle<()> = tokio::spawn(async move {
		if let Err(error) = server.run(shutdown_rx).await {
			error!("对局服务器运行失败：{error}");
		}
	});
	*running = Some(RunningServer { tcp_port, udp_port, ws_port, shutdown, task });
	Ok(())
}

/// 停止对局服务器。
///
/// 停止前会向仍连接的 UDP 客户端发送 `STOC::LEAVE_GAME`。
pub async fn shutdown() -> Result<(), Error> {
	let mut running: AsyncMutexGuard<'_, Option<RunningServer>> = SERVER.lock().await;
	if let Some(server) = running.take() {
		let _ = server.shutdown.send(());
		let _ = server.task.await;
	}
	Ok(())
}
