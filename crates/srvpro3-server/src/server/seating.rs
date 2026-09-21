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

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{Arc, Mutex};
	use ygopro::{player::BaseDuelPlayer, Configuration};
	use ygopro_data::message::HostInfo;
	use super::super::room::RoomRecord;

	fn room() -> (Duel, RecordConfig) {
		let mut duel = Duel::new(HostInfo { mode: Mode::Tag, ..Default::default() }, Configuration::default());
		duel.players.resize_with(4, || None);
		(duel, RecordConfig(Arc::new(Mutex::new(RoomRecord::new("test".into())))))
	}

	#[test]
	fn moves_player_and_host_without_changing_observers() {
		let (mut duel, config) = room();
		let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
		let (watch_sender, mut watch_receiver) = tokio::sync::mpsc::unbounded_channel();
		let mut player = BaseDuelPlayer::new(sender.clone());
		player.name = "host".into();
		duel.players[0] = Some(player.into());
		duel.players[0].as_mut().unwrap().deck.main.push(123);
		duel.sender.players.push(sender);
		duel.observers.insert(BaseDuelPlayer::new(watch_sender.clone()));
		duel.sender.observers.insert(watch_sender);
		duel.host_player = Netplayer::Player(0);
		let mut request = Request { message: ctos::HsToDuelist.into(), extra: Netplayer::Player(0) };
		let mut stop = StopFlag(false);
		let response = change_seat(&mut duel, &mut request, config, &mut stop).unwrap();
		assert!(stop.0);
		assert_eq!(request.extra, Netplayer::Player(1));
		assert_eq!(duel.host_player, request.extra);
		assert!(duel.players[0].is_none());
		assert_eq!(duel.players[1].as_ref().unwrap().deck.main, vec![123]);
		assert_eq!(duel.observers.len(), 1);
		assert!(matches!(response, stoc::Message::TypeChange(value) if value.player == request.extra && value.host));
		// 玩家和观战者均恰好收到离开旧座位、进入新座位两条广播。
		for receiver in [&mut receiver, &mut watch_receiver] {
			assert!(matches!(receiver.try_recv().unwrap().try_get().unwrap(), stoc::Message::HsPlayerChange(_)));
			assert!(matches!(receiver.try_recv().unwrap().try_get().unwrap(), stoc::Message::HsPlayerEnter(_)));
			assert!(receiver.try_recv().is_err());
		}
	}

	#[test]
	fn observer_request_is_left_to_original_handler() {
		let (mut duel, config) = room();
		let mut request = Request { message: ctos::HsToDuelist.into(), extra: Netplayer::Observer(0) };
		let mut stop = StopFlag(false);
		assert!(change_seat(&mut duel, &mut request, config, &mut stop).is_none());
		assert!(!stop.0);
		assert_eq!(request.extra, Netplayer::Observer(0));
		assert!(duel.players.iter().all(Option::is_none));
	}

	#[test]
	fn cycles_past_occupied_seats() {
		for (start, expected) in [(0, [2, 3, 0]), (1, [2, 3, 1])] {
			let mut players = [Some(0), Some(1), None, None];
			let mut current = start;
			for next in expected {
				assert_eq!(next_seat(&players, current), Some(next));
				players[next] = players[current].take();
				current = next;
			}
		}
	}

	#[test]
	fn full_room_cannot_change_seats() {
		for current in 0..4 {
			assert_eq!(next_seat(&[Some(()); 4], current), None);
		}
	}
}
