use std::{net::IpAddr, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use futures::{Stream, StreamExt};
use tokio::{sync::{mpsc, watch}, time::Instant};
use ygopro_data::{complex::Complex, constants::{Color, DuelStage, Netplayer}, data::Deck, message::{ctos, stoc, gm}};
use super::{decode::decode, room::RoomRecord, transport::Connection};
use crate::rooms::RoomList;

pub struct Handle {
	pub ip: IpAddr,
	pub name: String,
	pub pass: String,
	pub version: Option<u16>,
	pub available: Arc<AtomicBool>,
	pub sender: mpsc::Sender<Connection>,
}

pub fn version(connection: &Connection) -> Option<u16> {
	connection.initial.iter().find_map(|message| match message { ctos::Message::JoinGame(value) => Some(value.version), _ => None })
}

pub fn bytes(message: stoc::Message) -> Vec<u8> {
	let message: Complex<stoc::Message> = message.into();
	message.data.to_vec()
}

fn same_deck(left: &Deck, right: &Deck) -> bool {
	// CTOS 中主卡组和额外卡组是合并传输的；保留副卡组边界，忽略排列顺序。
	fn sorted(deck: &Deck) -> (Vec<u32>, Vec<u32>) {
		let mut main: Vec<_> = deck.main.iter().chain(&deck.extra).copied().collect();
		let mut side = deck.side.clone();
		main.sort_unstable();
		side.sort_unstable();
		(main, side)
	}
	sorted(left) == sorted(right)
}

fn connected(record: &Arc<Mutex<RoomRecord>>, rooms: &RoomList, room: &str, id: u64, value: bool) {
	let mut record = record.lock().unwrap();
	if let Some(player) = record.players.get_mut(&id) { player.connected = value; }
	record.update_info(rooms, room);
}

struct Live {
	incoming: mpsc::Receiver<Vec<u8>>,
	outgoing: mpsc::Sender<Vec<u8>>,
	verified: bool,
}

pub async fn run(
	connection: Connection,
	mut resumes: mpsc::Receiver<Connection>,
	available: Arc<AtomicBool>,
	engine_input: mpsc::Sender<ctos::Message>,
	mut engine_output: impl Stream<Item = Complex<stoc::Message>> + Unpin,
	mut finished: watch::Receiver<bool>,
	record: Arc<Mutex<RoomRecord>>,
	rooms: RoomList,
	room_id: String,
	id: u64,
	seconds: u64,
) {
	let mut live = Some(Live { incoming: connection.incoming, outgoing: connection.outgoing, verified: true });
	let mut deadline = None;
	let mut join_packet = None;
	let mut type_packet = None;
	let mut duel_number = 0;
	let mut closing = None;
	loop {
		if live.is_none() {
			if deadline.is_none() {
				let eligible = {
					let record = record.lock().unwrap();
					closing.is_none() && seconds > 0 && !matches!(record.stage, DuelStage::Begin | DuelStage::End)
						&& record.players.get(&id).is_some_and(|player| matches!(player.position, Netplayer::Player(_)) && player.reconnect_deck.is_some())
				};
				if !eligible { break; }
				deadline = Instant::now().checked_add(Duration::from_secs(seconds));
				if deadline.is_none() { break; }
				connected(&record, &rooms, &room_id, id, false);
			}
			available.store(closing.is_none(), Ordering::Release);
		}
		tokio::select! {
			message = engine_output.next() => {
				let Some(message) = message else { break };
				if let Ok(value) = message.try_get() {
					match value {
						stoc::Message::JoinGame(_) => join_packet = Some(message.data.to_vec()),
						stoc::Message::TypeChange(_) => type_packet = Some(message.data.to_vec()),
						_ => {}
					}
					let mut record = record.lock().unwrap();
					let slot = record.players.get(&id).and_then(|player| match player.position { Netplayer::Player(slot) => Some(slot), _ => None });
					let restoring = slot.is_some_and(|slot| record.refreshing.contains(&slot));
					// RequestField 的 Start 是场面快照，不是新的一局。
					if !(restoring && matches!(value, stoc::Message::GameMessage(value) if matches!(value.message, gm::Message::Start(_)))) {
						record.observe_output(id, &mut duel_number, value);
					}
					if matches!(value, stoc::Message::FieldFinish(_)) {
						if let Some(slot) = slot { record.refreshing.remove(&slot); }
					}
					if matches!(value, stoc::Message::TypeChange(_)) { record.update_info(&rooms, &room_id); }
				}
				if let Some(socket) = live.as_ref().filter(|socket| socket.verified) {
					// 慢连接也不能阻塞房间结束或保留座位计时。
					if socket.outgoing.try_send(message.data.to_vec()).is_err() {
						live = None;
					}
				}
			}
			_ = finished.changed(), if closing.is_none() => {
				// 允许引擎桥接任务转发最后的消息，但不无限等待后台任务退出。
				available.store(false, Ordering::Release);
				closing = Some(Instant::now() + Duration::from_secs(1));
			}
			_ = async {
				match closing { Some(time) => tokio::time::sleep_until(time).await, None => std::future::pending().await }
			} => {
				break;
			}
			_ = async {
				match deadline { Some(time) => tokio::time::sleep_until(time).await, None => std::future::pending().await }
			} => {
				break;
			}
			candidate = resumes.recv() => {
				let Some(candidate) = candidate else { break };
				if closing.is_some() || live.is_some() || deadline.is_none() { continue; }
				let (Some(join), Some(kind)) = (&join_packet, &type_packet) else { break };
				let outgoing = candidate.outgoing;
				if outgoing.try_send(join.clone()).is_err() || outgoing.try_send(kind.clone()).is_err() { continue; }
				let _ = outgoing.try_send(bytes(stoc::Chat { player: Color::Lightblue.into(), msg: "检测到断线座位，请提交原卡组以验证重连。".into() }.into()));
				live = Some(Live { incoming: candidate.incoming, outgoing, verified: false });
			}
			frame = async { live.as_mut().unwrap().incoming.recv().await }, if live.is_some() => {
				let Some(frame) = frame else { live = None; continue; };
				let Some(message) = decode(&frame) else { live = None; continue; };
				let socket = live.as_mut().unwrap();
				if !socket.verified {
					let ctos::Message::UpdateDeck(update) = message else {
						if matches!(message, ctos::Message::LeaveGame(_)) { live = None; }
						continue;
					};
					let restore = {
						let record = record.lock().unwrap();
						record.players.get(&id).and_then(|player| {
							let Netplayer::Player(slot) = player.position else { return None };
							let valid = player.reconnect_deck.as_ref().is_some_and(|deck| same_deck(deck, &update.deck));
							if !valid { return None; }
							record.engine.as_ref()?.upgrade().map(|engine| (slot, engine))
						})
					};
					let Some((slot, engine)) = restore else {
						let _ = socket.outgoing.try_send(bytes(stoc::Chat { player: Color::Red.into(), msg: "重连卡组校验失败。".into() }.into()));
						live = None;
						continue;
					};
					if engine.send(ygopro::duel::Request::Command { name: "srvpro_resume", arguments: Some(Box::new(slot)) }).is_err() { break; }
					socket.verified = true;
					deadline = None;
					connected(&record, &rooms, &room_id, id, true);
					continue;
				}
				if matches!(message, ctos::Message::LeaveGame(_)) { break; }
				{
					let mut record = record.lock().unwrap();
					record.observe_input(id, &message);
					if matches!(message, ctos::Message::Chat(_)) { record.update_info(&rooms, &room_id); }
				}
				if engine_input.try_send(message).is_err() { break; }
			}
		}
	}
	available.store(false, Ordering::Release);
	connected(&record, &rooms, &room_id, id, false);
	// 丢弃 engine_input 使 DuelHost 的长期桥接流结束，只在此时向引擎离场。
}
