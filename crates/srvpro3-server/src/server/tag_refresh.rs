use std::io::Cursor;
use binrw::BinRead;
use tokio::sync::mpsc::UnboundedSender;
use ygopro::{
	duel::PlayerIndex,
	plugin::time_limit::TimeLimit,
	tag_duel::{TagDuel, PlayerTransformer, ygopro_handlers::{Handler, TAG_DUEL_YGOPRO_HANDLERS}},
	ygocore_handlers::{Handler as CoreHandler, YGOCORE_HANDLERS},
};
use ygopro_data::{
	complex::Complex,
	constants::{CorePlayer, DuelStage, Location, Phase, Query},
	data::{QueryData, UpdateCardInfo},
	message::{ctos, stoc, gm::{self, GameMessage}},
};
use ygopro_derive::{Attachment, after, before, register_to};
use ygopro_handler::StopFlag;

pub const NAME: &str = module_path!();

#[derive(Attachment)]
pub struct FieldState {
	#[attachment(default = "Phase::Draw")]
	phase: Phase,
	deck_reversed: bool,
	turns: Vec<CorePlayer>,
}

#[after(gm::NewPhase)]
#[register_to(YGOCORE_HANDLERS as CoreHandler)]
fn phase(state: &mut FieldState, message: &gm::NewPhase) { state.phase = message.phase; }

#[after(gm::NewTurn)]
#[register_to(YGOCORE_HANDLERS as CoreHandler)]
fn turn(state: &mut FieldState, message: &gm::NewTurn) { state.turns.push(message.player); }

#[after(gm::ReverseDeck)]
#[register_to(YGOCORE_HANDLERS as CoreHandler)]
fn reverse(state: &mut FieldState) { state.deck_reversed = !state.deck_reversed; }

// 上游将未上场队友映射成 None(2)/Rule(5)，这只能用于选人，不能用于场面查询。
// 快照始终使用队伍对应的 0/1；发送目标仍是原座位，不改变实际行动玩家。
#[before(ctos::RequestField)]
#[register_to(TAG_DUEL_YGOPRO_HANDLERS)]
fn refresh(
	duel: &mut TagDuel,
	transformer: PlayerTransformer,
	player: PlayerIndex,
	state: &mut FieldState,
	timer: &mut TimeLimit,
	stop: &mut StopFlag,
) {
	stop.0 = true;
	if duel.stage != DuelStage::Dueling || duel.get(player).is_none() { return; }
	let sender: UnboundedSender<Complex<stoc::Message>> = duel.get(player).unwrap().stoc_sender.clone();
	let own: CorePlayer = transformer.team_to_core_player(player.team());
	let opponent: CorePlayer = own.opponent();
	let length: usize = duel.duel.core.query_field_info(&mut duel.duel.core_request_buffer) as usize;
	let mut cursor: Cursor<&[u8]> = Cursor::new(&duel.duel.core_request_buffer[..length]);
	let field: gm::Message = match gm::Message::read_le(&mut cursor) {
		Ok(field) => field,
		Err(error) => {
			srvpro3_log::error!("双打场面快照解析失败：{error}");
			let _ = sender.send(Complex::from_message(stoc::FieldFinish.into()));
			return;
		}
	};
	let mut messages: Vec<stoc::Message> = vec![stoc::DuelStart.into(), gm::Start {
		player_type: gm::PlayerType::new().with_player(own),
		rule: duel.host_info.duel_rule,
		player1_lp: duel.host_info.start_lp as i32,
		player2_lp: duel.host_info.start_lp as i32,
		player1_deck_count: 0, player1_extra_count: 0,
		player2_deck_count: 0, player2_extra_count: 0,
	}.into()];
	// 客户端根据 NewTurn 切换上场队友；只补 1～2 个回合会在后续轮换时显示错人。
	for &turn in &state.turns { messages.push(gm::NewTurn { player: turn }.into()); }
	messages.push(gm::NewPhase { phase: state.phase }.into());
	messages.push(field.into());
	for location in [Location::MZone, Location::SZone, Location::Hand, Location::Grave, Location::Extra, Location::Removed] {
		for mut message in duel.refresh_location(opponent, location, Query::all()) {
			if !duel.configuration.no_mask { message.mask(); }
			messages.push(message.into());
		}
		for message in duel.refresh_location(own, location, Query::all()) { messages.push(message.into()); }
	}
	if state.deck_reversed { messages.push(gm::ReverseDeck.into()); }
	for side in [CorePlayer::FirstAttackPlayer, CorePlayer::SecondAttackPlayer] {
		let gm::Message::UpdateData(update) = duel.query_location_cards(side, Location::Deck, Query::Code | Query::Position) else { continue };
		let Some(UpdateCardInfo::Data(data)) = update.data.last() else { continue };
		let Some(QueryData::Code(code)) = data.first() else { continue };
		let Some(QueryData::Position(position)) = data.get(1) else { continue };
		let public: bool = !position.position.is_face_down();
		if !state.deck_reversed && !public { continue; }
		messages.push(gm::DeckTop {
			player: side, sequence: 0,
			code: gm::CardCode::new().with_id(*code as u32).with_is_public(public),
		}.into());
	}
	if duel.host_info.time_limit > 0 {
		let waiting: CorePlayer = duel.last_response.map(|index: PlayerIndex| transformer.team_to_core_player(index.team())).unwrap_or(own);
		// 计时包的 player 同样只能是 0/1；最后发送正在计时的一方，保持客户端计时指示。
		for side in [waiting.opponent(), waiting] {
			let Some(index) = transformer.to_player_index(side) else { continue };
			let base: u16 = duel.get(index).map_or(0, |player| player.time_limit);
			let left_time: u16 = if Some(index) == duel.last_response {
				base.saturating_sub(timer.time_elapsed.min(u16::MAX as u32) as u16)
			} else { base };
			messages.push(stoc::TimeLimit { player: side, left_time }.into());
		}
	}
	messages.push(stoc::FieldFinish.into());
	if duel.last_response == Some(player) {
		if let Some(message) = &duel.last_select_message { messages.push(message.clone().into()); }
	}
	// 恢复包含本队手牌，不广播，也不混入观战补发和录像。
	for message in messages {
		if sender.send(Complex::from_message(message)).is_err() { break; }
	}
}
