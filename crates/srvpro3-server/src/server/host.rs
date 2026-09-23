use std::{any::Any, collections::VecDeque, pin::Pin, time::Duration};
use futures::{Stream, StreamExt};
use tokio::sync::{mpsc, oneshot, watch};
use ygopro::{Configuration, command::{CommandHandler, COMMANDS}, duel::{Duel, Request, SendTarget}, message::ClientJoin, single_duel::SingleDuel, tag_duel::TagDuel, ygopro_handlers::{Handler, Request as ClientRequest, RequestEx, YGOPRO_HANDLERS}};
use ygopro_data::{complex::Complex, constants::{Mode, Netplayer}, message::{HostInfo, ctos, stoc}};
use ygopro_derive::{Attachment, before, command, register_to};
use ygopro_handler::StopFlag;

pub const NAME: &str = module_path!();
const JOIN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING: usize = 64;
type OutputSender = mpsc::UnboundedSender<Complex<stoc::Message>>;

// 不使用上游 DuelHost::bridge：其输入 EOF 可能先于 TypeChange 被处理。
// 对局本身仍由上游 SingleDuel / TagDuel 运行。
pub struct DuelHost {
	input: mpsc::UnboundedSender<Request>,
	pub finished_sender: watch::Sender<bool>,
}

impl DuelHost {
	pub fn new(host_info: HostInfo, mut configuration: Configuration) -> Self {
		configuration.enable_plugin(NAME);
		let (input, handle) = if host_info.mode == Mode::Tag {
			let duel = TagDuel::new(host_info.clone(), configuration);
			(duel.request_sender.clone(), duel.run().expect("duel already started"))
		} else {
			let duel = SingleDuel { duel: Duel::new(host_info.clone(), configuration), first_attack_player: None, duel_winner: Vec::new() };
			(duel.request_sender.clone(), duel.run().expect("duel already started"))
		};
		let (finished_sender, _) = watch::channel(false);
		let finished = finished_sender.clone();
		tokio::spawn(async move {
			let _ = handle.await;
			finished.send_replace(true);
		});
		let _ = input.send(Request::Message(ClientRequest {
			message: ctos::CreateGame { host_info, name: "".into(), pass: "".into() }.into(),
			extra: Netplayer::Unknown,
		}));
		Self { input, finished_sender }
	}

	pub fn get_finish_signal(&self) -> impl Future<Output = ()> + Send + use<> {
		let mut finished = self.finished_sender.subscribe();
		async move { let _ = finished.wait_for(|value| *value).await; }
	}

	pub fn add(&self, input: impl Stream<Item = ctos::Message> + Unpin + Send + 'static) -> Pin<Box<dyn Stream<Item = Complex<stoc::Message>> + Send>> {
		let (output, receiver) = mpsc::unbounded_channel();
		let (sender, messages) = mpsc::unbounded_channel();
		let (position_sender, position) = oneshot::channel();
		let cleanup = Cleanup { input: self.input.clone(), sender: sender.clone() };
		let _ = self.input.send(Request::MessageEx(RequestEx {
			message: ClientJoin { stoc_sender: sender, position_sender: Some(position_sender) }.into(),
			extra: SendTarget::None,
		}));
		tokio::spawn(bridge(input, messages, output, position, self.finished_sender.subscribe(), cleanup));
		Box::pin(futures::stream::unfold(receiver, |mut receiver| async move {
			receiver.recv().await.map(|message| (message, receiver))
		}))
	}
}

// 即使任务被取消，也排队清理原连接。清理时在引擎线程内重新定位，不能使用旧座位号。
struct Cleanup {
	input: mpsc::UnboundedSender<Request>,
	sender: OutputSender,
}

impl Drop for Cleanup {
	fn drop(&mut self) {
		let _ = self.input.send(Request::Command { name: "srvpro_connection_leave", arguments: Some(Box::new(self.sender.clone())) });
	}
}

async fn bridge(
	mut input: impl Stream<Item = ctos::Message> + Unpin,
	mut messages: mpsc::UnboundedReceiver<Complex<stoc::Message>>,
	output: OutputSender,
	position: oneshot::Receiver<Netplayer>,
	mut finished: watch::Receiver<bool>,
	cleanup: Cleanup,
) {
	let expires = tokio::time::Instant::now() + JOIN_TIMEOUT;
	let mut position = tokio::select! {
		value = position => value.unwrap_or(Netplayer::Unknown),
		_ = finished.wait_for(|value| *value) => return,
		_ = tokio::time::sleep_until(expires) => {
			srvpro3_log::warn!("等待引擎分配连接身份超时，清理连接");
			return;
		}
	};
	let mut join_sent = false;
	let mut join_received = false;
	let mut type_received = false;
	let mut disconnected = false;
	let mut pending = VecDeque::new();
	let mut ended = false;
	loop {
		let joined = join_received && type_received;
		if disconnected && (joined || !join_sent) { break; }
		// 每次只提交一条，输出优先更新身份；队列始终保留原消息顺序。
		tokio::select! {
			biased;
			_ = finished.wait_for(|value| *value), if !ended => {
				// 引擎已经退出，不再产生输出；保留已排队的录像和观战补发消息。
				ended = true;
				messages.close();
			}
			_ = tokio::time::sleep_until(expires), if !joined => {
				srvpro3_log::warn!("等待 JoinGame / TypeChange 超时，清理连接");
				break;
			}
			message = messages.recv() => {
				let Some(message) = message else { break };
				let mut rejected = false;
				if let Ok(value) = message.try_get() {
					match value {
						stoc::Message::JoinGame(_) => join_received = true,
						stoc::Message::TypeChange(value) => {
							position = value.player;
							type_received = matches!(position, Netplayer::Player(_) | Netplayer::Observer(_));
						}
						stoc::Message::ErrorMessage(_) if !joined => rejected = true,
						stoc::Message::LeaveGame(value) if value.pos == position => break,
						_ => {}
					}
				}
				if output.send(message).is_err() {
					disconnected = true;
					pending.clear();
				}
				if rejected { break; }
			}
			message = async { pending.pop_front().unwrap() }, if joined && !ended && !pending.is_empty() => {
				if cleanup.input.send(Request::Message(ClientRequest { message, extra: position })).is_err() { break; }
			}
			message = input.next(), if !disconnected && !ended => {
				let Some(message) = message else { disconnected = true; pending.clear(); continue; };
				if matches!(message, ctos::Message::LeaveGame(_)) {
					disconnected = true;
					pending.clear();
					continue;
				}
				// 握手只能进行一次，避免重复 JoinGame 再次分配身份。
				if matches!(message, ctos::Message::CreateGame(_)) { continue; }
				let handshake = matches!(message, ctos::Message::PlayerInfo(_) | ctos::Message::JoinGame(_));
				if handshake && join_sent { continue; }
				if !joined && !handshake {
					if pending.len() >= MAX_PENDING {
						srvpro3_log::warn!("入房期间待处理消息过多，关闭连接并等待入房清理");
						disconnected = true;
						pending.clear();
					} else { pending.push_back(message); }
					continue;
				}
				if matches!(message, ctos::Message::JoinGame(_)) { join_sent = true; }
				if cleanup.input.send(Request::Message(ClientRequest { message, extra: position })).is_err() { break; }
			}
		}
	}
}

#[derive(Attachment)]
pub struct Leaves {
	pending: VecDeque<OutputSender>,
}

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_connection_leave(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>, leaves: &mut Leaves) {
	let Some(sender) = arguments.downcast_ref::<OutputSender>() else { return; };
	leaves.pending.push_back(sender.clone());
	duel.queue_request(ctos::LeaveGame, Netplayer::Unknown);
}

#[before(ctos::LeaveGame, priority = 128)]
#[register_to(YGOPRO_HANDLERS)]
fn leaving(duel: &mut Duel, request: &mut ClientRequest, leaves: &mut Leaves, stop: &mut StopFlag) {
	if request.extra != Netplayer::Unknown { return; }
	let Some(sender) = leaves.pending.pop_front() else { stop.0 = true; return; };
	let position = duel.players.iter().enumerate().find_map(|(index, player)| {
		player.as_ref().filter(|player| player.stoc_sender.same_channel(&sender)).map(|_| Netplayer::Player(index as u8))
	}).or_else(|| duel.observers.iter().find_map(|(index, player)| {
		player.stoc_sender.same_channel(&sender).then_some(Netplayer::Observer(index as u8))
	})).or_else(|| duel.uninit_players.iter().find_map(|(index, player)| {
		player.stoc_sender.same_channel(&sender).then_some(Netplayer::Undecided(index as u8))
	}));
	// 不存在说明已经被踢出或已移除，不得再按旧位置清理其他连接。
	if let Some(position) = position { request.extra = position; } else { stop.0 = true; }
}
