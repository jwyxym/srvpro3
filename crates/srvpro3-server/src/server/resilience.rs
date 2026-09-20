use std::{any::Any, time::Duration};
use ygopro::{command::{CommandHandler, COMMANDS}, duel::{Duel, Request, SendTarget}, message, player::AllowMessage, ygopro_handlers::{Handler, HandlerEx, YGOPRO_HANDLERS, YGOPRO_HANDLERS_EX}};
use ygopro_data::{constants::{Color, DuelStage, Netplayer}, message::{ctos, stoc}};
use ygopro_derive::{after, command, register_to, Attachment};
use super::recorder::RecordConfig;

pub const NAME: &str = module_path!();

#[derive(Attachment)]
pub struct SideTimer {
	task: Option<tokio::task::JoinHandle<()>>,
}

impl SideTimer {
	fn stop(&mut self) {
		if let Some(task) = self.task.take() { task.abort(); }
	}
}

impl Drop for SideTimer {
	fn drop(&mut self) { self.stop(); }
}

fn sync(duel: &Duel, config: &RecordConfig) {
	let mut record = config.0.lock().unwrap();
	record.stage = duel.stage;
	record.engine = Some(duel.request_sender.downgrade());
}

#[after(ctos::CreateGame)]
#[register_to(YGOPRO_HANDLERS)]
fn created(duel: &mut Duel, config: RecordConfig) { sync(duel, &config); }

#[after(ctos::HsStart)]
#[register_to(YGOPRO_HANDLERS)]
fn started(duel: &mut Duel, config: RecordConfig) { sync(duel, &config); }

#[after(ctos::HandResult)]
#[register_to(YGOPRO_HANDLERS)]
fn hand(duel: &mut Duel, config: RecordConfig) { sync(duel, &config); }

#[after(ctos::TpResult)]
#[register_to(YGOPRO_HANDLERS)]
fn turn(duel: &mut Duel, config: RecordConfig) { sync(duel, &config); }

#[after(ctos::UpdateDeck)]
#[register_to(YGOPRO_HANDLERS)]
fn deck(duel: &mut Duel, config: RecordConfig, timer: &mut SideTimer) {
	sync(duel, &config);
	if duel.stage != DuelStage::Siding { timer.stop(); }
}

#[after(message::DuelStart)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn duel_started(duel: &mut Duel, config: RecordConfig, timer: &mut SideTimer) {
	sync(duel, &config);
	timer.stop();
}

#[after(message::MatchEnd)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn ended(config: RecordConfig, timer: &mut SideTimer) {
	config.0.lock().unwrap().stage = DuelStage::End;
	timer.stop();
}

#[after(message::RecreateDuel)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn siding(duel: &mut Duel, config: RecordConfig, timer: &mut SideTimer) {
	sync(duel, &config);
	timer.stop();
	let seconds = config.0.lock().unwrap().side_timeout_secs;
	if seconds == 0 || duel.stage != DuelStage::Siding { return; }
	duel.sender.send(stoc::Chat { player: Color::Lightblue.into(), msg: format!("请在 {seconds} 秒内完成换副，超时将结束房间。").into() }.into(), SendTarget::All);
	let sender = duel.request_sender.downgrade();
	let round = duel.duel_count;
	timer.task = Some(tokio::spawn(async move {
		tokio::time::sleep(Duration::from_secs(seconds)).await;
		if let Some(sender) = sender.upgrade() {
			let _ = sender.send(Request::Command { name: "srvpro_side_timeout", arguments: Some(Box::new(round)) });
		}
	}));
}

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_side_timeout(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>, config: RecordConfig) -> &'static str {
	if arguments.downcast_ref::<u8>() != Some(&duel.duel_count) || duel.stage != DuelStage::Siding { return "continue"; }
	let overdue: Vec<_> = duel.players.iter().enumerate()
		.filter_map(|(slot, player)| player.as_ref().filter(|p| !p.ready).map(|_| slot as u8)).collect();
	if overdue.is_empty() { return "continue"; }
	if let Some(room) = config.0.lock().unwrap().tournament.as_mut() { room.aborted = true; }
	duel.sender.send(stoc::Chat { player: Color::Red.into(), msg: "换副超时，房间已结束。".into() }.into(), SendTarget::All);
	duel.sender.send(stoc::DuelEnd.into(), SendTarget::All);
	config.0.lock().unwrap().stage = DuelStage::End;
	"terminate"
}

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_interrupt(duel: &mut Duel, _: &mut Box<dyn Any + Send>, config: RecordConfig, timer: &mut SideTimer) -> &'static str {
	timer.stop();
	if let Some(room) = config.0.lock().unwrap().tournament.as_mut() { room.aborted = true; }
	duel.sender.send(stoc::Chat { player: Color::Red.into(), msg: "房间已被管理员中断。".into() }.into(), SendTarget::All);
	// 等待开局的房间无需通知对局结束；猜拳、选先后手和换副仍属于已开局。
	if matches!(duel.stage, DuelStage::Finger | DuelStage::Firstgo | DuelStage::Dueling | DuelStage::Siding) {
		duel.sender.send(stoc::DuelEnd.into(), SendTarget::All);
	}
	config.0.lock().unwrap().stage = DuelStage::End;
	"terminate"
}

// 仅服务器在卡组校验通过后调用；在引擎线程内读取状态，避免恢复旧快照。
#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_resume(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>, config: RecordConfig) -> &'static str {
	let Some(&slot) = arguments.downcast_ref::<u8>() else { return "continue" };
	let position = Netplayer::Player(slot);
	let Some(player) = duel.get_net(position) else { return "continue" };
	let ready = player.ready;
	let select_hand = matches!(player.state, AllowMessage::Some(ctos::MessageType::HandResult));
	let select_tp = matches!(player.state, AllowMessage::Some(ctos::MessageType::TpResult));
	let target = SendTarget::Single(position);
	duel.sender.send(stoc::TypeChange { player: position, host: duel.host_player == position }.into(), target.clone());
	for (index, player) in duel.players.iter().enumerate() {
		if let Some(player) = player {
			duel.sender.send(stoc::HsPlayerEnter { name: player.name.clone(), pos: Netplayer::Player(index as u8) }.into(), target.clone());
		}
	}
	match duel.stage {
		DuelStage::Dueling => {
			config.0.lock().unwrap().refreshing.insert(slot);
			let _ = duel.request_sender.send(Request::Message(ygopro::ygopro_handlers::Request { message: ctos::RequestField.into(), extra: position }));
		}
		DuelStage::Siding if !ready => duel.sender.send(stoc::ChangeSide.into(), target),
		DuelStage::Finger | DuelStage::Firstgo | DuelStage::Siding => {
			duel.sender.send(stoc::DuelStart.into(), target.clone());
			if select_hand { duel.sender.send(stoc::SelectHand.into(), target.clone()); }
			if select_tp { duel.sender.send(stoc::SelectTp.into(), target); }
		}
		_ => {}
	}
	"continue"
}
