use std::{collections::VecDeque, net::IpAddr, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
use futures::{Stream, StreamExt};
use tokio::{sync::{mpsc, watch}, time::Instant};
use ygopro_data::{complex::Complex, constants::{Color, DuelStage, Netplayer}, data::Deck, message::{ctos, stoc, gm}};
use super::{decode::decode, room::RoomRecord, transport::{Connection, Protocol}};
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

pub fn same_deck(left: &Deck, right: &Deck) -> bool {
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

fn disconnect_kicked_player(record: &Arc<Mutex<RoomRecord>>, kicker: u64, target: Netplayer) {
	let Netplayer::Player(_) = target else { return };
	let player = {
		let mut record = record.lock().unwrap();
		if record.stage != DuelStage::Begin || !record.players.get(&kicker).is_some_and(|player| player.is_host) {
			return;
		}
		record.players.values_mut().find(|player| {
			player.connected && player.position == target
		}).map(|player| {
			if player.protocol == Protocol::Udp {
				(player.protocol, player.outgoing.clone(), None)
			} else {
				(player.protocol, player.outgoing.clone(), player.close.take())
			}
		})
	};
	if let Some((protocol, outgoing, close)) = player {
		if protocol == Protocol::Udp {
			let _ = outgoing.try_send(bytes(stoc::LeaveGame { pos: target }.into()));
		} else if let Some(close) = close {
			let _ = close.send(());
		}
	}
}

struct Live {
	incoming: mpsc::Receiver<Vec<u8>>,
	outgoing: mpsc::Sender<Vec<u8>>,
	protocol: Protocol,
	close: Option<tokio::sync::oneshot::Sender<()>>,
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
	let mut live = Some(Live { incoming: connection.incoming, outgoing: connection.outgoing, protocol: connection.protocol, close: None, verified: true });
	let mut deadline = None;
	let mut join_packet = None;
	let mut type_packet = None;
	let mut duel_number = 0;
	let mut closing = None;
	let mut udp_leave_sent = false;
	let mut pending_observer: VecDeque<Vec<u8>> = VecDeque::new();
	let mut welcomed = false;
	loop {
		let observer = record.lock().unwrap().players.get(&id)
			.is_some_and(|player| matches!(player.position, Netplayer::Observer(_)));
		// 房间移除后引擎流中可能仍有观战补发消息，以流耗尽作为发送完成标志。
		if live.is_none() {
			pending_observer.clear();
			if deadline.is_none() {
				let eligible = {
					let record = record.lock().unwrap();
					closing.is_none() && seconds > 0 && record.stage == DuelStage::Dueling
						&& record.players.get(&id).is_some_and(|player| !player.is_bot && matches!(player.position, Netplayer::Player(_)) && player.reconnect_deck.is_some())
				};
				if !eligible { break; }
				deadline = Instant::now().checked_add(Duration::from_secs(seconds));
				if deadline.is_none() { break; }
				connected(&record, &rooms, &room_id, id, false);
			}
			available.store(closing.is_none(), Ordering::Release);
		}
		let observer_outgoing = live.as_ref().filter(|_| !pending_observer.is_empty()).map(|socket| socket.outgoing.clone());
		tokio::select! {
			// 队列拥堵时暂停读取后续补发消息，仍处理退出和房间结束通知。
			permit = async move {
				observer_outgoing.unwrap().reserve_owned().await
			}, if live.is_some() && !pending_observer.is_empty() => {
				match permit {
					Ok(permit) => {
						if let Some(frame) = pending_observer.pop_front() {
							permit.send(frame);
						}
					}
					Err(_) => live = None,
				}
			}
			message = engine_output.next(), if pending_observer.is_empty() => {
				let Some(mut message) = message else { break };
				if record.lock().unwrap().tournament.is_some() {
					// 赛事由服务器自动开局，主持人转移及重连时也不向客户端授予房主权限。
					if let Ok(stoc::Message::TypeChange(value)) = message.try_get() {
						message = Complex::from_message(stoc::TypeChange { player: value.player, host: false }.into());
					}
				}
				let mut leave_position = None;
				let mut welcome = Vec::new();
				if let Ok(value) = message.try_get() {
					if !welcomed && matches!(value, stoc::Message::JoinGame(_)) {
						welcomed = true;
						welcome = super::messages::welcome();
					}
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
					if matches!(value, stoc::Message::DuelEnd(_)) {
						leave_position = record.players.get(&id).map(|player| player.position);
					}
					if matches!(value, stoc::Message::TypeChange(_)) { record.update_info(&rooms, &room_id); }
				}
				if let Some(socket) = live.as_ref().filter(|socket| socket.verified) {
					let observer = record.lock().unwrap().players.get(&id)
						.is_some_and(|player| matches!(player.position, Netplayer::Observer(_)));
					if observer {
						// 逐条排队，UDP 的 LEAVE_GAME 留到整个输出流耗尽之后发送。
						pending_observer.push_back(message.data.to_vec());
						pending_observer.extend(welcome);
						continue;
					}
					// 慢连接也不能阻塞房间结束或保留座位计时。
					if socket.outgoing.try_send(message.data.to_vec()).is_err() {
						live = None;
					} else {
						for frame in welcome { if socket.outgoing.try_send(frame).is_err() { break; } }
						if socket.protocol == Protocol::Udp {
							if let Some(position) = leave_position {
								udp_leave_sent = socket.outgoing.try_send(bytes(stoc::LeaveGame { pos: position }.into())).is_ok();
							}
						}
					}
				}
			}
			_ = finished.changed(), if closing.is_none() => {
				// 普通玩家保留结束宽限期；观战者继续排空引擎消息流。
				available.store(false, Ordering::Release);
				closing = Some(Instant::now() + Duration::from_secs(1));
			}
			_ = async {
				match closing { Some(time) => tokio::time::sleep_until(time).await, None => std::future::pending().await }
			}, if !observer => {
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
				let protocol = candidate.protocol;
				let outgoing = candidate.outgoing;
				let close = candidate.close;
				if outgoing.try_send(join.clone()).is_err() || outgoing.try_send(kind.clone()).is_err() { continue; }
				let _ = outgoing.try_send(bytes(stoc::Chat { player: Color::Lightblue.into(), msg: "检测到断线座位，请提交原卡组以验证重连。".into() }.into()));
				live = Some(Live { incoming: candidate.incoming, outgoing, protocol, close, verified: false });
				udp_leave_sent = false;
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
					if let Some(player) = record.lock().unwrap().players.get_mut(&id) {
						player.protocol = socket.protocol;
						player.outgoing = socket.outgoing.clone();
						player.close = socket.close.take();
					}
					connected(&record, &rooms, &room_id, id, true);
					continue;
				}
				if matches!(message, ctos::Message::LeaveGame(_)) { break; }
				// 引擎已结束时不再转发观战输入，避免发送失败截断剩余输出。
				if observer && (closing.is_some() || !rooms.read().contains_key(&room_id)) { continue; }
				if record.lock().unwrap().tournament.is_some() && super::tournament::blocked_input(&message) {
					let _ = socket.outgoing.try_send(bytes(stoc::Chat { player: Color::Red.into(), msg: "比赛房间不允许更改座位、踢人或手动开局。".into() }.into()));
					continue;
				}
				if let ctos::Message::Chat(chat) = &message {
					if super::messages::command(&chat.msg, &socket.outgoing) { continue; }
					if super::bot::command(&chat.msg, &record, &room_id, id) { continue; }
				}
				let kicked = if let ctos::Message::HsKick(kick) = &message { Some(kick.pos) } else { None };
				{
					let mut record = record.lock().unwrap();
					record.observe_input(id, &message);
					if matches!(message, ctos::Message::Chat(_)) { record.update_info(&rooms, &room_id); }
				}
				if engine_input.try_send(message).is_err() { break; }
				if let Some(target) = kicked {
					disconnect_kicked_player(&record, id, target);
				}
			}
		}
	}
	available.store(false, Ordering::Release);
	// 先结束引擎输入并更新房间列表，不让 UDP 传输清理延迟玩家离房。
	// DuelHost 在输入流结束时发送 LeaveGame，移除座位并销毁空房间。
	drop(engine_input);
	connected(&record, &rooms, &room_id, id, false);
	// 引擎结束不等于传输连接结束：历史记录仍可能持有 outgoing，必须主动关闭。
	let player = {
		let mut record = record.lock().unwrap();
		record.players.get_mut(&id).map(|player| (
			player.protocol, player.position, player.outgoing.clone(), player.close.take(),
		))
	};
	if let Some((protocol, position, outgoing, close)) = player {
		if protocol == Protocol::Udp {
			if !udp_leave_sent {
				let leave = bytes(stoc::LeaveGame { pos: position }.into());
				if matches!(position, Netplayer::Observer(_)) {
					// 排在全部剩余消息后，不能因短暂队列拥堵丢掉离场通知。
					let _ = outgoing.send(leave).await;
				} else {
					let _ = tokio::time::timeout(Duration::from_millis(200), outgoing.send(leave)).await;
				}
			}
			// 为 KCP 留出发送离场消息的时间，避免关闭会话时丢掉待发送数据。
			tokio::time::sleep(Duration::from_millis(200)).await;
		}
		if let Some(close) = close { let _ = close.send(()); }
	}
	// 尚未完成身份验证的重连，其关闭信号还没有移交给 PlayerRecord。
	if let Some(socket) = live.as_mut() {
		if let Some(close) = socket.close.take() {
			if socket.protocol == Protocol::Udp {
				let _ = tokio::time::timeout(Duration::from_millis(200), socket.outgoing.send(bytes(stoc::LeaveGame { pos: Netplayer::Unknown }.into()))).await;
				tokio::time::sleep(Duration::from_millis(200)).await;
			}
			let _ = close.send(());
		}
	}
}
