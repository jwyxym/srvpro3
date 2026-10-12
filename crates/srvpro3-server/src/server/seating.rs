use ygopro::{duel::{Duel, SendTarget}, ygopro_handlers::{Handler, Request, YGOPRO_HANDLERS}};
use ygopro_data::{constants::{DuelStage, Mode, Netplayer, PlayerChange, PlayerChangeState}, message::{ctos, stoc}};
use ygopro_derive::{before, register_to};
use ygopro_handler::StopFlag;
use super::recorder::RecordConfig;

pub const NAME: &str = module_path!();

fn next_seat<T>(players: &[Option<T>], current: usize) -> Option<usize> {
	(1..players.len()).map(|offset| (current + offset) % players.len())
		.find(|&slot| players[slot].is_none())
}

#[before(ctos::HsToDuelist)]
#[register_to(YGOPRO_HANDLERS)]
fn change_seat(duel: &mut Duel, request: &mut Request, config: RecordConfig, stop: &mut StopFlag) -> Option<stoc::Message> {
	// 观战者仍由底层 HsToDuelist 处理，保留原有入座和观战人数通知。
	let Netplayer::Player(current) = request.extra else { return None };
	if duel.host_info.mode != Mode::Tag || duel.players.len() != 4 { return None; }
	stop.0 = true;
	if duel.stage != DuelStage::Begin { return None; }
	let player = duel.get_net(request.extra)?;
	if player.ready { return None; }
	let current = current as usize;
	let next = next_seat(&duel.players, current)?;
	let old_position = request.extra;
	let new_position = Netplayer::Player(next as u8);
	let is_host = duel.host_player == old_position;
	let player = duel.players[current].take()?;
	let name = player.name.clone();
	// 迁移完整玩家对象，保留卡组；同时迁移发送通道，避免旧座位重复收包。
	duel.sender.players.resize_with(4, || tokio::sync::mpsc::unbounded_channel().0);
	duel.sender.players[current] = tokio::sync::mpsc::unbounded_channel().0;
	duel.sender.players[next] = player.stoc_sender.clone();
	duel.players[next] = Some(player);
	request.extra = new_position;
	if is_host { duel.host_player = new_position; }
	{
		let mut record = config.0.lock().unwrap();
		record.store_accepted_deck(current as u8, None);
		record.store_accepted_deck(next as u8, None);
	}
	duel.sender.send(stoc::HsPlayerChange {
		status: PlayerChange::new().with_player(old_position).with_state(PlayerChangeState::Leave),
	}.into(), SendTarget::All);
	duel.sender.send(stoc::HsPlayerEnter { name, pos: new_position }.into(), SendTarget::All);
	Some(stoc::TypeChange { player: new_position, host: is_host }.into())
}
