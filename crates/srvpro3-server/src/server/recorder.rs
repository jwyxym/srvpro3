use std::sync::{Arc, Mutex};
use ygopro::{
	duel::{Duel, PlayerIndex},
	message::{DuelStart, DuelEnd, GenerateReplay},
	single_duel::{SingleDuel, ygopro_handlers::{HandlerEx, SINGLE_DUEL_YGOPRO_HANDLERS_EX}},
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
		winner_id: None,
		room_id: record.room_id.clone(),
		first_attack_slot: duel.first_attack_player.unwrap_or(PlayerIndex(0)).0,
	});
}

#[before(DuelEnd)]
#[register_to(SINGLE_DUEL_YGOPRO_HANDLERS_EX as HandlerEx)]
fn on_duel_end(message: &DuelEnd, config: RecordConfig) {
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
	let buffer = if replay { super::history::replay_buffer(duel).map(Some) } else { Ok(None) };
	tokio::spawn(async move {
		let result = async {
			let buffer = buffer?;
			super::history::persist(history, buffer).await
		}.await;
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
