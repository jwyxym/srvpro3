use super::server::Server;

use anyhow::{Error, anyhow};
use parking_lot::{Mutex, MutexGuard, RawRwLock, lock_api::RwLockReadGuard};
use std::{
	borrow::Cow,
	ffi::{CStr, c_char, c_int},
	fs::read,
	ptr::null_mut,
	sync::Arc
};
use ygopro_core_wrapper::{intptr_t, set_card_reader, set_message_handler, set_script_reader};
use ygopro_data::{
	constants::{Attribute, Category, Linkmarkers, OT, Race, Type},
	data::{CoreCard, Card, LFList},
};
use ygopro::managers::{
	config_manager::{ConfigManager, set_global as set_config_manager},
	data_manager::{DataManager, card_reader, set_global as set_data_manager},
	deck_manager::{DeckManager, set_global as set_deck_manager},
};

use srvpro3_config::Config;
use srvpro3_cards::CardsSnapshot;

static SCRIPT_BUFFER: Mutex<[u8; 0x100000]> = Mutex::new([0u8; 0x100000]);

extern "C" fn script_reader (script_path: *const c_char, slen: *mut c_int) -> *mut u8 {
	fn read_file(file_path: &str, buffer: &mut [u8]) -> Result<usize, Error> {
		let data: Vec<u8> = read(file_path)?;
		let len: usize = data.len();
		if len >= buffer.len() {
			Err(anyhow!("内存长度溢出"))
		} else {
			buffer[..len].copy_from_slice(&data);
			Ok(len)
		}
	}
	if script_path.is_null() || slen.is_null() {
		return null_mut();
	}
	let path: Cow<'_, str> = unsafe { CStr::from_ptr(script_path).to_string_lossy() };
	let mut buffer: MutexGuard<'_, [u8; 0x100000]> = SCRIPT_BUFFER.lock();

	if path.starts_with("./script") {
		(move || -> Result<*mut u8, Error> {
			let script_name: &str = &path[2..];
			let data: Vec<u8> = srvpro3_cards::script(script_name)?;
			let len: usize = data.len();
			if len >= buffer.len() {
				Err(anyhow!("内存长度溢出"))
			} else {
				buffer[..len].copy_from_slice(&data);
				unsafe {
					*slen = len as c_int;
				}
				Ok(buffer.as_mut_ptr())
			}
		})()
		.unwrap_or(null_mut())
	} else {
		(move || -> Result<*mut u8, Error> {
			let len: usize = read_file(path.as_ref(), &mut *buffer)?;
			unsafe {
				*slen = len as c_int;
			}
			Ok(buffer.as_mut_ptr())
		})()
		.unwrap_or(null_mut())
	}
}

extern "C" fn core_message_handler(_: intptr_t, _: u32) -> u32 {
	0
}

pub async fn init() -> Result<(), Error> {
	let mut data_manager: DataManager = DataManager::new();
	let cards: Arc<CardsSnapshot> = srvpro3_cards::get()?;
	for (_, i) in &cards.cards {
		let desc: [String; 16] = Default::default();
		data_manager.cards.insert(
			i.code,
			Card {
				card: CoreCard {
					code: i.code,
					alias: i.alias,
					setcode: i.setcode,
					card_type: Type::from_bits_retain(i.card_type),
					level: i.level,
					attribute: Attribute::from_bits_retain(i.attribute),
					race: Race::from_bits_retain(i.race),
					attack: i.attack,
					defense: i.defense,
					left_scale: i.lscale,
					right_scale: i.rscale,
					link_marker: Linkmarkers::from_bits_retain(i.link_marker),
					rule_code: 0,
				},
				ot: OT::from_bits_retain(i.ot),
				category: Category::from_bits_retain(i.category),
				name: String::new(),
				text: String::new(),
				desc
			},
		);
	}
	data_manager.finalize_db();
	let mut deck_manager: DeckManager = DeckManager::new();
	cards.lflists.iter().for_each(|(name, lflist)| {
		deck_manager.lflists.push(LFList {
			hash: lflist.hash,
			name: name.to_string(),
			content: lflist.lflist.clone(),
			genesys: lflist.genesys,
			glist: lflist.glist.clone()
		})
	});
	drop(cards);
	let config_manager: ConfigManager = ConfigManager::new();
	set_config_manager(config_manager);
	set_data_manager(data_manager);
	set_deck_manager(deck_manager);
	unsafe {
		set_script_reader(Some(script_reader));
		set_card_reader(Some(card_reader));
		set_message_handler(Some(core_message_handler));
	}
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()?;
	let tcp_port: u16 = config.server.tcp.port;
	let udp_port: u16 = config.server.udp.port;
	let ws_port: u16 = config.server.ws.port;
	let reconnect_timeout: u64 = config.server.reconnect_timeout;
	let side_timeout: u64 = config.server.side_timeout;
	drop(config);
	let server: Server = Server::bind(
		tcp_port,
		udp_port,
		ws_port,
		reconnect_timeout,
		side_timeout
	).await?;
	server.run().await
}
