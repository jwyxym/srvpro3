mod decode;
mod handshake;
mod recorder;
mod history;
mod password;
mod room;
mod player;
mod transport;
mod reconnect;
mod resilience;

use room::{RoomEntry, RoomRecord};
use player::PlayerRecord;

use super::rooms;

use std::{collections::BTreeMap, sync::{Arc, Mutex, atomic::AtomicBool}};
use parking_lot::{RawRwLock, lock_api::RwLock};
use anyhow::Error;
use futures::stream;
use tokio::{
	sync::{mpsc, oneshot, watch::Receiver},
	task::JoinSet,
	spawn
};
use ygopro::host::DuelHost;
use ygopro_data::{
	data::ReplayMode,
	constants::{CorePlayer, Netplayer},
	message::{ctos, stoc, gm},
	complex::Complex
};
use ygopro_handler::RoomProvider;

use srvpro3_log::*;


#[derive(Debug, Default)]
pub struct DuelRecord {
	pub winner: Option<gm::Win>,
	pub duel_players: BTreeMap<CorePlayer, Vec<u64>>,
}


pub struct Server {
	room_list: rooms::RoomList,
	listeners: Option<transport::Listeners>,
	connections: JoinSet<()>,
	rooms: BTreeMap<String, RoomEntry>,
	next_room_id: u64,
	next_connection_id: u64,
	reconnect_timeout_secs: u64,
	side_timeout_secs: u64,
	resumes: BTreeMap<u64, reconnect::Handle>,
	pub completed_rooms: Vec<Arc<Mutex<RoomRecord>>>,
	finished_tx: mpsc::UnboundedSender<String>,
	finished_rx: mpsc::UnboundedReceiver<String>,
	disconnected_tx: mpsc::UnboundedSender<(String, u64)>,
	disconnected_rx: mpsc::UnboundedReceiver<(String, u64)>,
	interrupt_rx: mpsc::UnboundedReceiver<rooms::Interrupt>,
}

impl Server {
	pub async fn bind(
		tcp_port: u16,
		udp_port:u16,
		ws_port: u16,
		reconnect_timeout: u64,
		side_timeout: u64
	) -> Result<Self, Error> {
		let listeners: transport::Listeners = transport::Listeners::bind(tcp_port, udp_port, ws_port).await?;
		let (finished_tx, finished_rx) = mpsc::unbounded_channel();
		let (disconnected_tx, disconnected_rx) = mpsc::unbounded_channel();
		let (interrupt_tx, interrupt_rx) = mpsc::unbounded_channel();
		rooms::register_control(interrupt_tx);
		Ok(Self {
			room_list: rooms::register(),
			listeners: Some(listeners),
			connections: JoinSet::new(),
			rooms: BTreeMap::new(),
			next_room_id: 0,
			next_connection_id: 0,
			reconnect_timeout_secs: reconnect_timeout,
			side_timeout_secs: side_timeout,
			resumes: BTreeMap::new(),
			completed_rooms: Vec::new(),
			finished_tx,
			finished_rx,
			disconnected_tx,
			disconnected_rx,
			interrupt_rx,
		})
	}

	pub async fn run(mut self, mut shutdown: oneshot::Receiver<()>) -> Result<(), Error> {
		let (ready_tx, mut ready_rx) = mpsc::channel(64);
		let mut listeners = self.listeners
			.take()
			.expect("监听器已经启动")
			.start(ready_tx);
		if listeners.is_empty() {
			error!("所有对局协议均已禁用");
			let _ = shutdown.await;
			return Ok(());
		}
		loop {
			tokio::select! {
				_ = &mut shutdown => {
					self.leave_udp_clients().await;
					return Ok(());
				}
				result = listeners.join_next() => {
					if let Some(result) = result { result??; }
					return Err(anyhow::anyhow!("传输监听任务意外退出"));
				}
				result = self.connections.join_next(), if !self.connections.is_empty() => {
					if let Some(Err(error)) = result { error!("房间连接任务异常：{error}"); }
				}
				Some(room_id) = self.finished_rx.recv() => {
					if let Some(room) = self.rooms.remove(&room_id) {
						if let Some(info) = self.room_list.write().remove(&room_id) {
							rooms::closed(info);
						}
						let records = room.record.lock().unwrap().history.clone();
						spawn(async move {
							if let Err(error) = history::persist(records).await {
								error!("房间 {room_id} 对局记录写入失败: {error:#}");
							}
						});
						self.completed_rooms.push(room.record);
					}
				}
				Some((room_id, id)) = self.disconnected_rx.recv() => {
					self.resumes.remove(&id);
					if let Some(room) = self.rooms.get_mut(&room_id) {
						if !room.record.lock().unwrap().players.contains_key(&id) { continue; }
						room.connections = room.connections.saturating_sub(1);
						room.record.lock().unwrap().update_info(&self.room_list, &room_id);
					}
				}
				Some(request) = self.interrupt_rx.recv() => {
					let _ = request.result.send(self.interrupt_room(&request.room_id));
				}
				Some(connection) = ready_rx.recv() => {
					self.accept_connection(connection)?;
				}
			}
		}
	}

	/// 向当前所有仍连接的 UDP 客户端发送离开房间消息。
	///
	/// 发送后由调用方继续关闭对应的传输服务。
	pub async fn leave_udp_clients(&self) {
		for room in self.rooms.values() {
			let record = room.record.lock().unwrap();
			for player in record.players.values() {
				if player.connected && player.protocol == transport::Protocol::Udp {
					let _ = player.outgoing.try_send(reconnect::bytes(
						stoc::LeaveGame { pos: player.position }.into()
					));
				}
			}
		}
		tokio::task::yield_now().await;
	}

	fn interrupt_room(&mut self, room_id: &str) -> bool {
		let Some(room) = self.rooms.get(room_id) else { return false };
		let engine = room.record.lock().unwrap().engine.as_ref().and_then(|engine| engine.upgrade());
		let Some(engine) = engine else { return false };
		engine.send(ygopro::duel::Request::Command { name: "srvpro_interrupt", arguments: None }).is_ok()
	}

	fn accept_connection(&mut self, connection: transport::Connection) -> Result<(), Error> {
		let version: Option<u16> = reconnect::version(&connection);
		let candidates: Vec<_> = self.resumes.values().filter(|handle| {
			handle.available.load(std::sync::atomic::Ordering::Acquire)
				&& handle.ip == connection.peer_ip && handle.name == connection.handshake.name
				&& handle.pass == connection.handshake.pass && handle.version == version
		}).collect();
		if !candidates.is_empty() {
			if candidates.len() == 1 {
				let handle = candidates[0];
				if handle.available.swap(false, std::sync::atomic::Ordering::AcqRel) {
					if handle.sender.try_send(connection).is_err() {
						handle.available.store(true, std::sync::atomic::Ordering::Release);
					}
				}
			} else {
				warn!("同 IP、名字和房间存在多个断线座位，拒绝不明确的重连");
			}
			return Ok(());
		}
		let connection_id: u64 = self.next_connection_id;
		self.next_connection_id = self.next_connection_id.wrapping_add(1);
		
		let options: password::Password = match password::Password::parse(&connection.handshake.pass) {
			Ok(options) => options,
			Err(error) => {
				warn!("拒绝不支持的房间模式：{error}");
				if connection.protocol == transport::Protocol::Udp {
					let _ = connection.outgoing.try_send(reconnect::bytes(stoc::LeaveGame { pos: Netplayer::Unknown }.into()));
				}
				let _ = connection.close.send(());
				return Ok(());
			}
		};
		let room_id: String = options.room_key.clone().unwrap_or_else(|| {
			self.random_room(&options.random_prefix(), options.capacity())
		});
		let add_auto_bot: bool = options.auto_bot && !self.rooms.contains_key(&room_id);
		let bot_password: String = connection.handshake.pass.clone();

		let room_id_for_finish: String = room_id.clone();
		let record_room_id: String = connection.handshake.pass.clone();
		let room: &mut RoomEntry = self.rooms.entry(room_id.clone()).or_insert_with(|| {
			let record: Arc<Mutex<RoomRecord>> = Arc::new(Mutex::new(RoomRecord::new(record_room_id)));
			record.lock().unwrap().team_size = (options.capacity() / 2) as u8;
			record.lock().unwrap().side_timeout_secs = self.side_timeout_secs;
			let mut configuration: ygopro::Configuration = ygopro::Configuration::default();
			options.configure(&mut configuration);
			configuration.enable_plugin(resilience::NAME);
			configuration.enable_plugin_with_configuration(recorder::NAME, recorder::RecordConfig(record.clone()));
			configuration.enable_plugin_with_configuration(ygopro::plugin::replay::NAME, ygopro::plugin::replay::Configuration { mode: ReplayMode::empty() });
			let mut host = DuelHost::new(options.host_info, configuration);
			let finish_signal =
				<DuelHost as RoomProvider<ctos::Message, Complex<stoc::Message>>>::get_finish_signal(&mut host);
			let finished_tx: mpsc::UnboundedSender<String> = self.finished_tx.clone();
			let room_id_for_finish = room_id_for_finish.clone();
			tokio::spawn(async move {
				finish_signal.await;
				let _ = finished_tx.send(room_id_for_finish);
			});
			RoomEntry {
				host,
				connections: 0,
				record,
			}
		});
		room.connections += 1;
		let added = {
			let mut rooms = self.room_list.write();
			if rooms.contains_key(&room_id) {
				None
			} else {
				let info = rooms::RoomInfo {
					room_id: room_id.clone(),
					connections: room.connections,
					player_a: Vec::new(),
					player_b: Vec::new(),
					spectators: 0,
					chats: Vec::new(),
				};
				rooms.insert(room_id.clone(), info.clone());
				Some(info)
			}
		};
		if let Some(info) = added { rooms::added(info); }
		let record = room.record.clone();
		record.lock().unwrap().players.insert(connection_id, PlayerRecord {
			name: connection.handshake.name.clone(),
			position: Netplayer::Unknown,
			is_host: false,
			connected: true,
			protocol: connection.protocol,
			outgoing: connection.outgoing.clone(),
			deck: None,
			reconnect_deck: None,
		});
		record.lock().unwrap().update_info(&self.room_list, &room_id);

		let (engine_input, engine_receiver) = mpsc::channel(64);
		for message in &connection.initial {
			engine_input.try_send(message.clone())?;
		}
		let input = stream::unfold(engine_receiver, |mut receiver| async move {
			receiver.recv().await.map(|message| (message, receiver))
		});
		let output = room.host.add(Box::pin(input));
		let finished: Receiver<bool> = room.host.finished_sender.subscribe();
		let (resume_sender, resume_receiver) = mpsc::channel(1);
		let available = Arc::new(AtomicBool::new(false));
		self.resumes.insert(connection_id, reconnect::Handle {
			ip: connection.peer_ip,
			name: connection.handshake.name.clone(),
			pass: connection.handshake.pass.clone(),
			version,
			available: available.clone(),
			sender: resume_sender,
		});
		let disconnected_tx: mpsc::UnboundedSender<(String, u64)> = self.disconnected_tx.clone();
		let room_list: Arc<RwLock<RawRwLock, BTreeMap<String, rooms::RoomInfo>>> = self.room_list.clone();
		let seconds: u64 = self.reconnect_timeout_secs;
		self.connections.spawn(async move {
			reconnect::run(connection, resume_receiver, available, engine_input, output, finished,
				record, room_list, room_id.clone(), connection_id, seconds).await;
			let _ = disconnected_tx.send((room_id, connection_id));
		});
		if add_auto_bot {
			let tcp_port: u16 = srvpro3_config::get()?.server.tcp.port;
			tokio::spawn(async move {
				let selected = match tokio::task::spawn_blocking(srvpro3_windbot::random).await {
					Ok(Ok(bot)) => bot,
					Ok(Err(error)) => {
						error!("获取随机 WindBot 失败：{error:#}");
						return;
					}
					Err(error) => {
						error!("获取随机 WindBot 任务失败：{error}");
						return;
					}
				};
				let bot = srvpro3_windbot::Bot {
					name: selected.name,
					deck: selected.ai_name,
					host: "127.0.0.1".to_owned(),
					port: tcp_port,
					password: bot_password,
					dialog: Some(selected.dialog),
					version: None,
					hand: None,
					debug: false,
					chat: true,
				};
				if let Err(error) = srvpro3_windbot::add(bot).await {
					error!("添加随机 WindBot 失败：{error:#}");
				}
			});
		}
		Ok(())
	}

	fn random_room(&mut self, prefix: &str, capacity: usize) -> String {
		if let Some((room_id, _)) = self.rooms.iter().find(|(room_id, room)| {
			room_id.starts_with(prefix) && room.connections < capacity
		}) {
			return room_id.clone();
		}

		loop {
			let room_id: String = format!("{prefix}{}", self.next_room_id);
			self.next_room_id = self.next_room_id.wrapping_add(1);
			if !self.rooms.contains_key(&room_id) {
				return room_id;
			}
		}
	}
}
