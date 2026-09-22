use std::sync::{Arc, Mutex};
use ygopro::{
	duel::{Duel, PlayerIndex},
	message::{DuelStart, DuelEnd, GenerateReplay},
	single_duel::{SingleDuel, ygopro_handlers::{HandlerEx, SINGLE_DUEL_YGOPRO_HANDLERS_EX}},
	tag_duel::{TagDuel, TeamIndex, ygopro_handlers::{HandlerEx as TagHandlerEx, TAG_DUEL_YGOPRO_HANDLERS_EX}},
	ygopro_handlers::{Handler, HandlerEx as GeneralHandlerEx, Request, YGOPRO_HANDLERS, YGOPRO_HANDLERS_EX},
};
use ygopro_data::{constants::{CorePlayer, Netplayer}, message::ctos};
use ygopro_derive::{after, before, register_to};
use ygopro_handler::{Bundle, FromRequest, extract::ContainsMap};
use super::{RoomRecord, history::HistoryRecord};

pub const NAME: &str = module_path!();

#[before(DuelStart)]
#[register_to(SINGLE_DUEL_YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_duel_start(duel: &mut SingleDuel, config: RecordConfig) {
	let (Some(a), Some(b)) = (duel.get(PlayerIndex(0)), duel.get(PlayerIndex(1))) else { return };
	let mut record = config.0.lock().unwrap();
	record.current_history = Some(HistoryRecord {
		player_a: a.name.to_string(),
		player_b: b.name.to_string(),
		deck_a: a.deck.to_string(),
		deck_b: b.deck.to_string(),
		player_c: None,
		player_d: None,
		deck_c: None,
		deck_d: None,
		winner_id: None,
		room_id: record.room_id.clone(),
		first_attack_slot: duel.first_attack_player.unwrap_or(PlayerIndex(0)).0,
	});
}

#[before(DuelEnd)]
#[register_to(SINGLE_DUEL_YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_duel_end(message: &DuelEnd, config: RecordConfig) {
	set_winner(message, config);
}

#[before(DuelStart)]
#[register_to(TAG_DUEL_YGOPRO_HANDLERS_EX as TagHandlerEx)]
fn on_tag_start(duel: &mut TagDuel, config: RecordConfig) {
	let (Some(a), Some(a_tag), Some(b), Some(b_tag)) = (
		duel.get(PlayerIndex(0)), duel.get(PlayerIndex(1)),
		duel.get(PlayerIndex(2)), duel.get(PlayerIndex(3)),
	) else { return };
	let mut record = config.0.lock().unwrap();
	record.current_history = Some(HistoryRecord {
		player_a: a.name.to_string(), player_b: b.name.to_string(),
		deck_a: a.deck.to_string(), deck_b: b.deck.to_string(),
		player_c: Some(a_tag.name.to_string()), player_d: Some(b_tag.name.to_string()),
		deck_c: Some(a_tag.deck.to_string()), deck_d: Some(b_tag.deck.to_string()),
		winner_id: None, room_id: record.room_id.clone(),
		// 历史结果按 A/B 队存储，与单打的 0/1 结果兼容。
		first_attack_slot: duel.first_attack_team.unwrap_or(TeamIndex::Team1).leader().0 / 2,
	});
}

#[before(DuelEnd)]
#[register_to(TAG_DUEL_YGOPRO_HANDLERS_EX as TagHandlerEx)]
fn on_tag_end(message: &DuelEnd, config: RecordConfig) {
	set_winner(message, config);
}

fn set_winner(message: &DuelEnd, config: RecordConfig) {
	let mut record = config.0.lock().unwrap();
	if let Some(history) = record.current_history.as_mut() {
		history.winner_id = match message.winner {
			CorePlayer::FirstAttackPlayer => Some(history.first_attack_slot),
			CorePlayer::SecondAttackPlayer => Some(history.first_attack_slot ^ 1),
			_ => None,
		};
	}
}

#[after(GenerateReplay)]
#[register_to(YGOPRO_HANDLERS_EX as GeneralHandlerEx)]
fn on_generate_replay(duel: &mut Duel, config: RecordConfig) {
	let history = {
		let mut record = config.0.lock().unwrap();
		let Some(history) = record.current_history.take() else { return; };
		// 留下每局结果供赛事比分统计；数据库写入只在此处触发一次。
		record.history.push(history.clone());
		history
	};
	let Ok(settings) = srvpro3_config::get() else { return; };
	if matches!(settings.db.db, srvpro3_config::DB::None) { return; }
	let replay = settings.server.replay;
	drop(settings);
	let buffer = if replay {
		match super::history::replay_buffer(duel) {
			Ok(buffer) => Some(buffer),
			Err(error) => {
				srvpro3_log::error!("生成录像失败，仍保存对局结果：{error:#}");
				None
			}
		}
	} else { None };
	tokio::spawn(async move {
		let result = super::history::persist(history, buffer).await;
		if let Err(error) = result { srvpro3_log::error!("保存小局历史记录失败：{error:#}"); }
	});
}

#[derive(Clone)]
pub struct RecordConfig(pub Arc<Mutex<RoomRecord>>);

impl<Req: Send, State: Send + ContainsMap, Res: Send> FromRequest<Req, State, Res> for RecordConfig {
	fn from_request(bundle: &mut Bundle<Req, State, Res>) -> Option<Self> {
		bundle.state.get_map().get::<Self>().cloned()
	}
}

fn save_ready_deck(duel: &Duel, index: PlayerIndex, config: &RecordConfig) {
	let Some(player) = duel.get(index) else { return };
	// 首局 HsReady 成功、或换备 UpdateDeck 成功之后，内部才将 ready 设为 true。
	if player.ready {
		config.0.lock().unwrap().store_accepted_deck(index.0, Some(player.deck.clone()));
	}
}

#[after(ctos::HsReady)]
#[register_to(YGOPRO_HANDLERS)]
fn on_ready(duel: &mut Duel, index: PlayerIndex, config: RecordConfig) {
	save_ready_deck(duel, index, &config);
}

#[after(ctos::UpdateDeck)]
#[register_to(YGOPRO_HANDLERS)]
fn on_update_deck(duel: &mut Duel, index: PlayerIndex, config: RecordConfig) {
	save_ready_deck(duel, index, &config);
}

#[after(ctos::HsNotReady)]
#[register_to(YGOPRO_HANDLERS)]
fn on_not_ready(index: PlayerIndex, config: RecordConfig) {
	config.0.lock().unwrap().store_accepted_deck(index.0, None);
}

#[after(ctos::JoinGame)]
#[register_to(YGOPRO_HANDLERS)]
fn on_join(request: &mut Request, config: RecordConfig) {
	// 新玩家占用旧座位时，不继承离开者的卡组。
	if let Netplayer::Player(slot) = request.extra {
		config.0.lock().unwrap().store_accepted_deck(slot, None);
	}
}
