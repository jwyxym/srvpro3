pub mod api;
pub mod plugin;

use std::sync::Arc;
use anyhow::{Result, anyhow, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use unicode_normalization::UnicodeNormalization;
use ygopro_data::{constants::DuelStage, data::Deck, message::ctos};
use super::{password::Password, room::RoomRecord, transport::Connection};
use api::{Api, Match, Participant, Score};

pub struct Admission {
	pub requested_at: tokio::time::Instant,
	pub api: Arc<Api>,
	pub match_info: Match,
	pub participants: [Participant; 2],
	pub slot: usize,
	pub deck: Option<Deck>,
	pub options: Password,
}

#[derive(Debug)]
pub struct Room {
	pub api: Arc<Api>,
	pub match_info: Match,
	pub participants: [Participant; 2],
	/// 连接 ID 由服务端分配，不能使用客户端自行填写的座位。
	pub seats: [Option<u64>; 2],
	pub decks: [Option<Deck>; 2],
	pub score: Option<Score>,
	pub forfeit: Option<usize>,
	pub aborted: bool,
}

impl Admission {
	pub fn room_id(&self) -> String {
		room_id(self.api.config.bo, self.match_info.id)
	}

	pub fn report_key(&self) -> String {
		self.api.report_key(self.match_info.id)
	}

	pub fn create_room(&self) -> Room {
		Room {
			api: self.api.clone(), match_info: self.match_info.clone(), participants: self.participants.clone(),
			seats: [None, None], decks: [None, None], score: None, forfeit: None, aborted: false,
		}
	}

	pub fn check_room(&self, record: &RoomRecord, reserved: impl Fn(u64) -> bool) -> Result<()> {
		let room = record.tournament.as_ref().ok_or_else(|| anyhow!("赛事房间编号与普通房间冲突"))?;
		ensure!(room.api.config == self.api.config && room.match_info.id == self.match_info.id
			&& room.match_info.player1_id == self.match_info.player1_id && room.match_info.player2_id == self.match_info.player2_id,
			"赛事配置或排表已经改变，请先中止旧房间");
		ensure!(record.stage == DuelStage::Begin, "这场比赛已经开始，请使用原来的名字和房间密码重连");
		if let Some(id) = room.seats[self.slot] {
			ensure!(!reserved(id) && !record.players.get(&id).is_some_and(|player| player.connected), "请不要重复加入比赛房间");
		}
		Ok(())
	}

	pub fn prepare(&self, connection: &mut Connection, id: u64) -> Result<()> {
		// 只转交已校验的两条握手，防止首包中的 CreateGame 等消息覆盖赛事规则。
		let name = connection.initial.iter().rev().find_map(|message| match message {
			ctos::Message::PlayerInfo(value) => Some(value.clone()), _ => None,
		}).ok_or_else(|| anyhow!("握手缺少玩家信息"))?;
		let mut join = connection.initial.iter().rev().find_map(|message| match message {
			ctos::Message::JoinGame(value) => Some(value.clone()), _ => None,
		}).ok_or_else(|| anyhow!("握手缺少加入房间消息"))?;
		ensure!(join.version == *ygopro::plugin::version_check::PRO_VERSION, "客户端协议版本与比赛服务器不符");
		// 该值只在服务端内部传给引擎，原始 pass 保留用于断线重连校验。
		join.pass = id.to_string().into();
		connection.initial = vec![name.into(), join.into()];
		Ok(())
	}
}

pub async fn resolve(name: &str, pass: &str, existing: Option<Arc<Api>>) -> Result<Option<Admission>> {
	let requested_at = tokio::time::Instant::now();
	let config = match &existing {
		Some(api) => api.config.clone(),
		None => srvpro3_config::get()?.tournament.clone(),
	};
	if !config.enabled && existing.is_none() { return Ok(None); }
	// 非空密码仍走正常房间规则；只有赛事预设房间号需要查询排表验证。
	let target = if pass.is_empty() {
		None
	} else {
		let options = Password::parse(pass)?;
		let Some(key) = options.room_key else { return Ok(None) };
		let number = key.rsplit('#').next().and_then(|value| value.parse::<u64>().ok());
		let Some(id) = number.filter(|id| room_id(config.bo, *id) == key) else { return Ok(None) };
		Some(id)
	};
	ensure!(!config.token.trim().is_empty(), "赛事模式未配置 token");
	ensure!(config.tournament_id > 0, "赛事模式未配置有效的比赛 ID");
	ensure!((1..=253).contains(&config.bo) && config.bo % 2 == 1, "赛事BO局数必须为 1 到 253 的奇数");
	ensure!((1..=60).contains(&config.timeout), "赛事接口超时时间必须为 1 到 60 秒");
	let options = Password::parse(&format!("BO{}#", config.bo))?;
	let pinned = existing.is_some();
	let api = match existing { Some(api) => api, None => Arc::new(Api::new(config)?) };
	let tournament = api.get().await?;
	ensure!(tournament.id == api.config.tournament_id, "排表接口返回了其他比赛的信息");
	let target_match = if let Some(id) = target {
		match tournament.matches.iter().map(|value| &value.match_info).find(|value| value.id == id) {
			Some(value) => Some(value),
			None if !pinned => return Ok(None),
			None => return Err(anyhow!("该赛事房间已不在当前排表中")),
		}
	} else { None };
	let mut participants = tournament.participants.iter().map(|value| &value.participant)
		.filter(|player| !player.quit && name_matches(&player.name, name));
	let player = participants.next().ok_or_else(|| anyhow!("你不是这场比赛的参赛选手，请检查报名昵称"))?;
	ensure!(participants.next().is_none(), "报名昵称对应多个参赛选手，请联系裁判处理");
	// 与 srvpro2 一致：pending/open 都可匹配，但双方选手必须已经确定。
	let playable = |value: &Match| matches!(value.state.as_str(), "pending" | "open") && value.winner_id.is_none()
		&& value.player1_id.is_some_and(|id| id > 0) && value.player2_id.is_some_and(|id| id > 0);
	let plays = |value: &Match| value.player1_id == Some(player.id) || value.player2_id == Some(player.id);
	let match_info = if let Some(value) = target_match {
		ensure!(plays(value), "你的报名昵称与该比赛房间的选手不符，拒绝连接");
		ensure!(playable(value), "这场比赛选手尚未确定、已经结束或为轮空对局");
		value.clone()
	} else {
		let mut matches = tournament.matches.iter().map(|value| &value.match_info).filter(|value| playable(value) && plays(value));
		let value = matches.next().ok_or_else(|| anyhow!("你没有当前轮次的有效比赛，可能尚未排表、轮空或比赛已经结束"))?.clone();
		ensure!(matches.next().is_none(), "当前排表存在多场未结束的比赛，请联系裁判处理");
		value
	};
	ensure!(match_info.player1_id != match_info.player2_id, "排表中的两位参赛选手不能相同");
	let participant = |id| -> Result<Participant> {
		tournament.participants.iter().map(|value| &value.participant)
			.find(|value| Some(value.id) == id && !value.quit).cloned().ok_or_else(|| anyhow!("对手未报名或已经退赛"))
	};
	let participants = [participant(match_info.player1_id)?, participant(match_info.player2_id)?];
	let slot = usize::from(match_info.player2_id == Some(player.id));
	let deck = if api.config.check_deck {
		Some(decode_deck(player.deckbuf.as_deref().ok_or_else(|| anyhow!("尚未登记比赛卡组，请联系裁判"))?)?)
	} else { None };
	Ok(Some(Admission { requested_at, api, match_info, participants, slot, deck, options }))
}

fn room_id(bo: u8, match_id: u64) -> String {
	match bo {
		1 => match_id.to_string(),
		3 => format!("M#{match_id}"),
		_ => format!("BO{bo}#{match_id}"),
	}
}

impl Room {
	pub fn report(&self, history: &[super::history::HistoryRecord]) -> Option<(Arc<Api>, u64, Score)> {
		if !self.api.config.post_score || self.aborted { return None; }
		let score = self.score.clone().or_else(|| {
			// 猜拳或换副时离场，引擎不一定产生 MatchEnd，同样按弃权处理。
			let winner = self.forfeit?;
			let mut scores = [0u32; 2];
			for game in history {
				if let Some(slot @ 0..=1) = game.winner_id { scores[slot as usize] += 1; }
			}
			scores[winner] = scores[winner].max(u32::from(self.api.config.bo / 2 + 1));
			Some(Score { scores_csv: format!("{}-{}", scores[0], scores[1]), winner_id: api::Winner::Player(self.participants[winner].id) })
		})?;
		Some((self.api.clone(), self.match_info.id, score))
	}
}

// 与 srvpro2 的报名卡组名称规则兼容，包括 NFC、.ydk 和 名字+编号。
fn name_matches(registered: &str, name: &str) -> bool {
	let mut base = registered;
	let mut extensions = 0;
	while base.get(base.len().saturating_sub(4)..).is_some_and(|end| end.eq_ignore_ascii_case(".ydk")) {
		base = &base[..base.len() - 4];
		extensions += 1;
	}
	let registered: String = if extensions > 2 { registered } else { base }.nfc().collect();
	let name: String = name.nfc().collect();
	if registered == name { return true; }
	if extensions > 2 { return false; }
	let parts: Vec<_> = registered.split(['+', '＋']).collect();
	parts.len() == 2 && parts.iter().all(|part| !part.is_empty()) && parts.contains(&name.as_str())
}

fn decode_deck(value: &str) -> Result<Deck> {
	ensure!(value.len() <= 4096, "登记卡组数据过大");
	let bytes = STANDARD.decode(value).map_err(|_| anyhow!("登记卡组不是有效的 Base64"))?;
	ensure!(bytes.len() >= 8 && bytes.len() % 4 == 0, "登记卡组不是有效的 UpdateDeck 数据");
	let words: Vec<u32> = bytes.chunks_exact(4).map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap())).collect();
	let main = words[0] as usize;
	let side = words[1] as usize;
	ensure!(main > 0 && main <= 512 && side <= 512 && words.len() == main + side + 2, "登记卡组的卡片数量无效");
	Ok(Deck { main: words[2..main + 2].to_vec(), extra: Vec::new(), side: words[main + 2..].to_vec() })
}

pub fn blocked_input(message: &ctos::Message) -> bool {
	matches!(message, ctos::Message::CreateGame(_) | ctos::Message::PlayerInfo(_) | ctos::Message::JoinGame(_)
		| ctos::Message::HsKick(_) | ctos::Message::HsToObserver(_) | ctos::Message::HsToDuelist(_) | ctos::Message::HsStart(_))
}
