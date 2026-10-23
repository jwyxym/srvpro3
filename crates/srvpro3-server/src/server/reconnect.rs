use std::{collections::VecDeque, net::IpAddr, sync::{Arc, Mutex, MutexGuard, atomic::{AtomicBool, Ordering}}, time::Duration};
use futures::{Stream, StreamExt};
use tokio::{sync::{mpsc, oneshot, watch}, time::Instant};
use ygopro::duel::Request;
use ygopro_data::{complex::Complex, constants::{Color, DuelStage, Netplayer}, data::Deck, message::{ctos, stoc, gm}};
use super::{decode::decode, player::PlayerRecord, room::RoomRecord, transport::{Connection, Protocol}};
use crate::rooms::RoomList;

pub struct Handle {
	pub ip: IpAddr,
	pub name: String,
	pub pass: String,
	pub version: Option<u16>,
	pub room_id: String,
	pub resuming: Arc<AtomicBool>,
	pub sender: mpsc::Sender<Connection>,
}

pub fn eligible(record: &RoomRecord, id: u64) -> bool {
	matches!(record.stage, DuelStage::Finger | DuelStage::Firstgo | DuelStage::Dueling | DuelStage::Siding)
		&& record.players.get(&id).is_some_and(|player| !player.is_bot
			&& matches!(player.position, Netplayer::Player(_)) && player.reconnect_deck.is_some())
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
	incoming: Option<mpsc::Receiver<Vec<u8>>>,
	outgoing: mpsc::Sender<Vec<u8>>,
	protocol: Protocol,
	close: Option<oneshot::Sender<()>>,
}

fn close_transport(protocol: Protocol, outgoing: &mpsc::Sender<Vec<u8>>, close: Option<oneshot::Sender<()>>, position: Netplayer) {
	if protocol == Protocol::Udp {
		let _ = outgoing.try_send(bytes(stoc::LeaveGame { pos: position }.into()));
		if let Some(close) = close {
			tokio::spawn(async move {
				tokio::time::sleep(Duration::from_millis(200)).await;
				let _ = close.send(());
			});
		}
	} else if let Some(close) = close {
		let _ = close.send(());
	}
}

impl Drop for Live {
	fn drop(&mut self) {
		if self.close.is_some() {
			close_transport(self.protocol, &self.outgoing, self.close.take(), Netplayer::Unknown);
		}
	}
}

struct Pending {
	socket: Live,
	initial: VecDeque<ctos::Message>,
	deadline: Instant,
	resuming: Arc<AtomicBool>,
}

impl Drop for Pending {
	fn drop(&mut self) { self.resuming.store(false, Ordering::Release); }
}

pub async fn run(
	connection: Connection,
	mut resumes: mpsc::Receiver<Connection>,
	resuming: Arc<AtomicBool>,
	engine_input: mpsc::Sender<ctos::Message>,
	mut engine_output: impl Stream<Item = Complex<stoc::Message>> + Unpin,
	mut finished: watch::Receiver<bool>,
	record: Arc<Mutex<RoomRecord>>,
	rooms: RoomList,
	room_id: String,
	id: u64,
	seconds: u64,
) {
	let mut live: Option<Live> = Some(Live { incoming: Some(connection.incoming), outgoing: connection.outgoing, protocol: connection.protocol, close: None });
	let mut pending: Option<Pending> = None;
	let mut deadline = None;
	let mut join_packet = None;
	let mut type_packet = None;
	let mut duel_number = 0;
	let mut closing = None;
	let mut udp_leave_sent = false;
	let mut pending_observer: VecDeque<Vec<u8>> = VecDeque::new();
	let mut welcomed = false;
	let mut joined = false;
	loop {
		let observer = record.lock().unwrap().players.get(&id)
			.is_some_and(|player| matches!(player.position, Netplayer::Observer(_)));
		// 房间移除后引擎流中可能仍有观战补发消息，以流耗尽作为发送完成标志。
		if live.is_none() {
			pending_observer.clear();
			if deadline.is_none() {
				if closing.is_some() || seconds == 0 || !eligible(&record.lock().unwrap(), id) { break; }
				deadline = Instant::now().checked_add(Duration::from_secs(seconds));
				if deadline.is_none() { break; }
				connected(&record, &rooms, &room_id, id, false);
			}
		}
		let observer_outgoing = live.as_ref().filter(|_| !pending_observer.is_empty()).map(|socket| socket.outgoing.clone());
		let verification_deadline: Option<Instant> = pending.as_ref().map(|attempt| attempt.deadline);
		// 先排完旧连接正在生成的快照，避免旧 FieldFinish 提前释放新连接的刷新状态。
		let can_verify: bool = pending.is_some() && {
			let record: MutexGuard<'_, RoomRecord> = record.lock().unwrap();
			!record.players.get(&id).is_some_and(|player: &PlayerRecord| {
				matches!(player.position, Netplayer::Player(slot) if record.refreshing.contains(&slot))
			})
		};
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
					// 等引擎确认加入并分配身份后通知；后续换位不重复发送。
					if !joined && welcomed && record.players.get(&id).is_some_and(|player| {
						matches!(player.position, Netplayer::Player(_) | Netplayer::Observer(_))
					}) {
						joined = true;
						super::messages::membership(&record, id, true);
					}
				}
				if let Some(socket) = live.as_ref() {
					let observer = record.lock().unwrap().players.get(&id)
						.is_some_and(|player| matches!(player.position, Netplayer::Observer(_)));
					if observer {
						// 逐条排队，UDP 的 LEAVE_GAME 留到整个输出流耗尽之后发送。
						pending_observer.push_back(message.data.to_vec());
						pending_observer.extend(welcome);
						continue;
					}
					// 慢连接也不能阻塞房间结束或保留座位计时。
					if let Err(error) = socket.outgoing.try_send(message.data.to_vec()) {
						srvpro3_log::warn!("房间 {room_id} 连接 {id} 转发引擎消息失败：opcode={:?}，{error}", message.data.first());
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
				pending = None;
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
				if closing.is_some() || pending.is_some() || !eligible(&record.lock().unwrap(), id) {
					super::Server::reject_connection(candidate);
					if pending.is_none() { resuming.store(false, Ordering::Release); }
					continue;
				}
				let (Some(join), Some(kind)) = (&join_packet, &type_packet) else {
					super::Server::reject_connection(candidate);
					resuming.store(false, Ordering::Release);
					continue;
				};
				// 验证期间继续服务旧连接，校验通过前不替换座位或关闭旧连接。
				let attempt: Pending = Pending {
					socket: Live { incoming: Some(candidate.incoming), outgoing: candidate.outgoing, protocol: candidate.protocol, close: candidate.close },
					initial: candidate.initial.into_iter().filter(|message| matches!(message, ctos::Message::UpdateDeck(_))).collect(),
					deadline: Instant::now() + Duration::from_secs(seconds),
					resuming: resuming.clone(),
				};
				if attempt.socket.outgoing.try_send(join.clone()).is_err() || attempt.socket.outgoing.try_send(kind.clone()).is_err() { continue; }
				let _ = attempt.socket.outgoing.try_send(bytes(stoc::Chat { player: Color::Lightblue.into(), msg: "检测到原玩家座位，请提交原卡组以验证重连。".into() }.into()));
				srvpro3_log::info!("房间 {room_id} 连接 {id} 开始验证重连：旧连接在线={}", live.is_some());
				pending = Some(attempt);
			}
			_ = async { tokio::time::sleep_until(verification_deadline.unwrap()).await }, if verification_deadline.is_some() => {
				srvpro3_log::info!("房间 {room_id} 连接 {id} 重连卡组验证超时");
				pending = None;
			}
			message = async {
				let attempt: &mut Pending = pending.as_mut().unwrap();
				if let Some(message) = attempt.initial.pop_front() { return Some(message); }
				attempt.socket.incoming.as_mut().unwrap().recv().await.and_then(|frame| decode(&frame))
			}, if can_verify => {
				let Some(message) = message else { pending = None; continue; };
				let ctos::Message::UpdateDeck(update) = message else {
					if matches!(message, ctos::Message::LeaveGame(_)) { pending = None; }
					continue;
				};
				let restore: Option<(u8, mpsc::UnboundedSender<Request>)> = {
					let record: MutexGuard<'_, RoomRecord> = record.lock().unwrap();
					if !eligible(&record, id) { None } else {
						record.players.get(&id).and_then(|player| {
							let Netplayer::Player(slot) = player.position else { return None };
							if !player.reconnect_deck.as_ref().is_some_and(|deck| same_deck(deck, &update.deck)) { return None; }
							record.engine.as_ref()?.upgrade().map(|engine| (slot, engine))
						})
					}
				};
				let Some((slot, engine)) = restore else {
					let _ = pending.as_ref().unwrap().socket.outgoing.try_send(bytes(stoc::Chat { player: Color::Red.into(), msg: "重连卡组校验失败。".into() }.into()));
					srvpro3_log::info!("房间 {room_id} 连接 {id} 重连卡组校验失败，保留旧座位");
					pending = None;
					continue;
				};
				let mut attempt: Pending = pending.take().unwrap();
				{
					let mut record: MutexGuard<'_, RoomRecord> = record.lock().unwrap();
					let player: &mut PlayerRecord = record.players.get_mut(&id).unwrap();
					let old_close: Option<oneshot::Sender<()>> = player.close.take();
					close_transport(player.protocol, &player.outgoing, old_close, player.position);
					player.protocol = attempt.socket.protocol;
					player.outgoing = attempt.socket.outgoing.clone();
					player.close = attempt.socket.close.take();
					// 在引擎执行恢复命令前占用刷新状态，合并新连接立即发来的 RequestField。
					record.refreshing.insert(slot);
				}
				// 保持原引擎通道和连接 ID；旧传输退出不能触发原玩家离房。
				live = Some(Live { incoming: attempt.socket.incoming.take(), outgoing: attempt.socket.outgoing.clone(), protocol: attempt.socket.protocol, close: None });
				deadline = None;
				udp_leave_sent = false;
				connected(&record, &rooms, &room_id, id, true);
				if engine.send(Request::Command { name: "srvpro_resume", arguments: Some(Box::new(slot)) }).is_err() { break; }
				srvpro3_log::info!("房间 {room_id} 连接 {id} 重连成功，已接管原座位 Player({slot})");
			}
			frame = async { live.as_mut().unwrap().incoming.as_mut().unwrap().recv().await }, if live.is_some() => {
				let Some(frame) = frame else { live = None; continue; };
				let Some(message) = decode(&frame) else { live = None; continue; };
				let socket = live.as_mut().unwrap();
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
					if matches!(message, ctos::Message::RequestField(_)) {
						let slot: Option<u8> = record.players.get(&id).and_then(|player: &PlayerRecord| {
							match player.position { Netplayer::Player(slot) => Some(slot), _ => None }
						});
						let Some(slot) = slot else { continue; };
						// 一次快照发送到 FieldFinish 前只接受一个请求，普通手动刷新也走同一规则。
						if record.stage != DuelStage::Dueling || !record.refreshing.insert(slot) { continue; }
					}
					record.observe_input(id, &message);
					if matches!(message, ctos::Message::Chat(_)) { record.update_info(&rooms, &room_id); }
				}
				if let Err(error) = engine_input.try_send(message) {
					srvpro3_log::warn!("房间 {room_id} 连接 {id} 转发客户端消息失败：{error}");
					break;
				}
				if let Some(target) = kicked {
					disconnect_kicked_player(&record, id, target);
				}
			}
		}
	}
	drop(pending);
	resuming.store(true, Ordering::Release);
	// 保留座位期间不会走到这里；房间整体结束时不逐个刷离房提示。
	if joined && !*finished.borrow() && rooms.read().contains_key(&room_id) {
		super::messages::membership(&record.lock().unwrap(), id, false);
	}
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
}
