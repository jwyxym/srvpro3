use std::{any::Any, ops::Deref};
use ygopro::{
	command::{CommandHandler, COMMANDS},
	duel::{Duel, Request},
	message,
	player::BaseDuelPlayer,
	ygopro_handlers::{Handler, HandlerEx, Response, YGOPRO_HANDLERS, YGOPRO_HANDLERS_EX},
};
use ygopro_data::{constants::{DuelStage, Netplayer}, message::{ctos, gm::{self, GameMessage}, stoc}};
use ygopro_derive::{after, command, register_to};

pub const NAME: &str = module_path!();

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_spectate(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>) {
	let Some(Netplayer::Observer(index)) = arguments.downcast_ref::<Netplayer>().copied() else { return; };
	let Some(sender) = duel.sender.observers.get(index as usize) else { return; };
	// 先通知客户端离开等待房间；GAME_MSG START 本身不会切换对局界面。
	if sender.send(stoc::Message::from(stoc::DuelStart).into()).is_err() { return; }
	// 从最新一局的 START 开始，使用已遮蔽手牌等私有信息的观战消息。
	let start = duel.sender.masked_messages.iter().rposition(|message| {
		matches!(message.deref(), stoc::Message::GameMessage(game) if matches!(&game.message, gm::Message::Start(start) if start.player_type & 0x10 != 0))
	});
	let Some(start) = start else { return; };
	for message in duel.sender.masked_messages.iter().skip(start) {
		let stoc::Message::GameMessage(game) = message.deref() else { continue; };
		// 观战者不接收要求选卡、操作或响应的消息。
		if game.message.waiting_for().is_some() { continue; }
		// 直接发往该观战者，不广播，也不将补发消息重复记入历史。
		if sender.send(message.clone()).is_err() { break; }
	}
}

#[after(ctos::JoinGame)]
#[register_to(YGOPRO_HANDLERS)]
fn joined(duel: &mut Duel, player: Netplayer) {
	if duel.stage > DuelStage::Begin && matches!(player, Netplayer::Observer(_)) {
		// 排在正常进房响应之后，发送一次 DUEL_START，再补发 GAME_MSG。
		let _ = duel.request_sender.send(Request::Command {
			name: "srvpro_spectate", arguments: Some(Box::new(player)),
		});
	}
}

#[after(message::ClientJoin)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn client_join(duel: &mut Duel, join: &mut message::ClientJoin, response: &mut Response) {
	if duel.stage <= DuelStage::Begin { return; }
	let Some(position_sender) = join.position_sender.take() else { return; };
	*response = Response::Continue;
	let player = BaseDuelPlayer::new(join.stoc_sender.clone());
	let index = duel.uninit_players.insert(player);
	duel.sender.undecided.insert(join.stoc_sender.clone());
	let _ = position_sender.send(Netplayer::Undecided(index as u8));
}
