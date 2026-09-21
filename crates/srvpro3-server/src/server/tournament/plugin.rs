use ygopro::{
	duel::{Duel, PlayerIndex, SendTarget},
	message,
	managers::deck_manager,
	single_duel::{SingleDuel, ygopro_handlers::{HandlerEx, SINGLE_DUEL_YGOPRO_HANDLERS_EX}},
	ygopro_handlers::{Handler, Request, YGOPRO_HANDLERS},
};
use ygopro_data::{constants::{Color, DuelStage, ErrorMessage, JoinError, Netplayer, PlayerChange, PlayerChangeState}, data::DeckError, message::{ctos, stoc}};
use ygopro_derive::{after, before, register_to};
use ygopro_handler::StopFlag;
use super::api::{Score, Winner};
use super::super::{recorder::RecordConfig, reconnect::same_deck};

pub const NAME: &str = module_path!();

// 覆盖默认的先到先坐：排表中的 player1/player2 始终对应座位 0/1。
#[before(ctos::JoinGame)]
#[register_to(YGOPRO_HANDLERS)]
fn join(duel: &mut Duel, request: &mut Request, config: RecordConfig, stop: &mut StopFlag) -> Result<Vec<stoc::Message>, stoc::Message> {
	stop.0 = true;
	let refused = || stoc::Message::from(stoc::ErrorMessage { err: ErrorMessage::JoinError(JoinError::HostRefused) });
	let ctos::Message::JoinGame(join) = &request.message else { return Err(refused()) };
	if join.version != *ygopro::plugin::version_check::PRO_VERSION {
		return Err(stoc::ErrorMessage { err: ErrorMessage::VersionError(*ygopro::plugin::version_check::PRO_VERSION) }.into());
	}
	let id: u64 = join.pass.to_string().parse().map_err(|_| refused())?;
	let Netplayer::Undecided(undecided) = request.extra else { return Err(refused()) };
	let mut record = config.0.lock().unwrap();
	let room = record.tournament.as_ref().ok_or_else(refused)?;
	if super::watchable(duel.stage) && !room.seats.contains(&Some(id)) {
		if !srvpro3_config::get().is_ok_and(|config| config.server.watch)
			|| !record.players.get(&id).is_some_and(|player| player.connected && player.position == Netplayer::Unknown) {
			return Err(refused());
		}
		let observer = duel.observers.vacant_entry().key();
		let observer = u8::try_from(observer).map_err(|_| refused())?;
		let player = duel.uninit_players.try_remove(undecided as usize).ok_or_else(refused)?;
		duel.sender.undecided.remove(undecided as usize);
		let sender = player.stoc_sender.clone();
		duel.observers.insert(player);
		duel.sender.observers.insert(sender);
		let position = Netplayer::Observer(observer);
		request.extra = position;
		if let Some(player) = record.players.get_mut(&id) {
			player.position = position;
			player.is_host = false;
		}
		drop(record);
		let mut info = duel.host_info.clone();
		if let Some(list) = deck_manager::load().get_lflist_by_index(info.lflist) { info.lflist = list.hash; }
		let mut response = vec![stoc::JoinGame { info }.into(), stoc::TypeChange { player: position, host: false }.into()];
		for (slot, player) in duel.players.iter().enumerate() {
			if let Some(player) = player {
				response.push(stoc::HsPlayerEnter { name: player.name.clone(), pos: Netplayer::Player(slot as u8) }.into());
			}
		}
		// 本插件设置了 StopFlag，普通 JoinGame 的 after 钩子不会执行。
		// 命令排在进房响应之后，切换对局界面并补发观战进度。
		let _ = duel.request_sender.send(ygopro::duel::Request::Command {
			name: "srvpro_spectate", arguments: Some(Box::new(position)),
		});
		return Ok(response);
	}
	let slot = room.seats.iter().position(|value| *value == Some(id)).ok_or_else(refused)?;
	if duel.stage != DuelStage::Begin || duel.players[slot].is_some() { return Err(refused()); }
	let player = duel.uninit_players.try_remove(undecided as usize).ok_or_else(refused)?;
	duel.sender.undecided.remove(undecided as usize);
	let position = Netplayer::Player(slot as u8);
	let creator = duel.players.iter().all(Option::is_none);
	if creator { duel.host_player = position; }
	let name = player.name.clone();
	let sender = player.stoc_sender.clone();
	duel.players[slot] = Some(player.into());
	duel.sender.players.resize_with(2, || tokio::sync::mpsc::unbounded_channel().0);
	duel.sender.players[slot] = sender;
	request.extra = position;
	if let Some(player) = record.players.get_mut(&id) {
		player.position = position;
		player.is_host = false;
	}
	record.store_accepted_deck(slot as u8, None);
	drop(record);
	duel.sender.send(stoc::HsPlayerEnter { name, pos: position }.into(), SendTarget::Except(position));
	let mut info = duel.host_info.clone();
	if let Some(list) = deck_manager::load().get_lflist_by_index(info.lflist) { info.lflist = list.hash; }
	let mut response = vec![stoc::JoinGame { info }.into(), stoc::TypeChange { player: position, host: false }.into()];
	for (slot, player) in duel.players.iter().enumerate() {
		if let Some(player) = player {
			let position = Netplayer::Player(slot as u8);
			response.push(stoc::HsPlayerEnter { name: player.name.clone(), pos: position }.into());
			if player.ready {
				response.push(stoc::HsPlayerChange { status: PlayerChange::new().with_player(position).with_state(PlayerChangeState::Ready) }.into());
			}
		}
	}
	response.push(stoc::Chat { player: Color::Lightblue.into(), msg: "已进入比赛专用房间，双方准备后自动开始。".into() }.into());
	Ok(response)
}

#[before(ctos::HsReady)]
#[register_to(YGOPRO_HANDLERS)]
fn check_deck(duel: &mut Duel, index: PlayerIndex, config: RecordConfig, stop: &mut StopFlag) -> Option<Vec<stoc::Message>> {
	if duel.stage != DuelStage::Begin { return None; }
	let record = config.0.lock().unwrap();
	let room = record.tournament.as_ref()?;
	if !room.api.config.check_deck { return None; }
	let valid = room.decks.get(index.0 as usize).and_then(Option::as_ref)
		.zip(duel.get(index)).is_some_and(|(deck, player)| same_deck(deck, &player.deck));
	if valid { return None; }
	stop.0 = true;
	// 与 srvpro2 一致，仅回复该玩家：聊天提示、取消准备、卡组错误（完整 code 为 0）。
	Some(vec![
		stoc::Chat { player: Color::Red.into(), msg: "卡组与报名登记的卡组不一致，无法准备。".into() }.into(),
		stoc::HsPlayerChange {
			status: PlayerChange::new().with_player(Netplayer::Player(index.0)).with_state(PlayerChangeState::Notready),
		}.into(),
		stoc::ErrorMessage { err: ErrorMessage::DeckError(DeckError::from_bytes([0; 4])) }.into(),
	])
}

#[after(ctos::HsReady)]
#[register_to(YGOPRO_HANDLERS)]
fn ready(duel: &mut Duel) {
	if duel.stage == DuelStage::Begin && duel.players.iter().all(|player| player.as_ref().is_some_and(|player| player.ready)) {
		let _ = duel.request_sender.send(ygopro::duel::Request::Message(Request { message: ctos::HsStart.into(), extra: duel.host_player }));
	}
}

#[before(ctos::HsStart)]
#[register_to(YGOPRO_HANDLERS)]
fn start(duel: &mut Duel, stop: &mut StopFlag) {
	// 重复的准备消息可能排入多个自动开局请求，已经开局后不能再次开始。
	if duel.stage != DuelStage::Begin { stop.0 = true; }
}

#[before(ctos::LeaveGame)]
#[register_to(YGOPRO_HANDLERS)]
fn leaving(duel: &mut Duel, index: PlayerIndex, config: RecordConfig) {
	if matches!(duel.stage, DuelStage::Begin | DuelStage::End) || duel.get(index).is_none() { return; }
	if let Some(room) = config.0.lock().unwrap().tournament.as_mut() {
		if !room.aborted && room.score.is_none() { room.forfeit.get_or_insert((index.0 ^ 1) as usize); }
	}
}

#[after(ctos::LeaveGame)]
#[register_to(YGOPRO_HANDLERS)]
fn left_between_duels(duel: &mut Duel, config: RecordConfig) -> &'static str {
	if duel.stage == DuelStage::Begin {
		// 默认引擎转移房主时会取消准备，赛事客户端不显示房主，也需同步准备状态。
		for (slot, player) in duel.players.iter().enumerate() {
			if let Some(player) = player {
				let state = if player.ready { PlayerChangeState::Ready } else { PlayerChangeState::Notready };
				duel.sender.send(stoc::HsPlayerChange { status: PlayerChange::new().with_player(Netplayer::Player(slot as u8)).with_state(state) }.into(), SendTarget::All);
			}
		}
		return "continue";
	}
	// 猜拳、选先后手和换副阶段没有运行中的 core，不能依赖 core 发出结束信号。
	if !matches!(duel.stage, DuelStage::Finger | DuelStage::Firstgo | DuelStage::Siding) { return "continue"; }
	let mut record = config.0.lock().unwrap();
	if !record.tournament.as_ref().is_some_and(|room| !room.aborted && room.forfeit.is_some()) { return "continue"; }
	record.stage = DuelStage::End;
	duel.sender.send(stoc::DuelEnd.into(), SendTarget::All);
	"terminate"
}

#[after(message::MatchEnd)]
#[register_to(SINGLE_DUEL_YGOPRO_HANDLERS_EX as HandlerEx)]
fn finished(duel: &mut SingleDuel, config: RecordConfig) {
	let mut record = config.0.lock().unwrap();
	let Some(room) = record.tournament.as_mut() else { return };
	if room.aborted || (duel.duel_winner.is_empty() && room.forfeit.is_none()) { return; }
	let mut scores = [0u32; 2];
	for winner in duel.duel_winner.iter().flatten() { scores[winner.0 as usize] += 1; }
	let forced = room.forfeit.or_else(|| {
		(duel.match_kill_card_code > 0).then(|| duel.duel_winner.last().copied().flatten().map(|slot| slot.0 as usize)).flatten()
	});
	if let Some(slot) = forced { scores[slot] = scores[slot].max(u32::from(room.api.config.bo / 2 + 1)); }
	let winner = forced.or_else(|| match scores[0].cmp(&scores[1]) {
		std::cmp::Ordering::Greater => Some(0), std::cmp::Ordering::Less => Some(1), _ => None,
	});
	room.score = Some(Score {
		scores_csv: format!("{}-{}", scores[0], scores[1]),
		winner_id: winner.map(|slot| Winner::Player(room.participants[slot].id)).unwrap_or_else(|| Winner::Tie("tie".into())),
	});
}
