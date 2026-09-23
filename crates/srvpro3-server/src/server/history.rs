use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use base64::{Engine, engine::general_purpose::STANDARD};
use binrw::BinWrite;
use flate2::{Compression, write::GzEncoder};
use ygopro_data::message::{stoc, gm::{self, GameMessage}};

pub const REPLAY_PREFIX: &str = "yrp3d:v1:";

/// 每局开始时固定玩家和卡组，避免换备、断线及换座位影响历史记录。
#[derive(Clone, Debug)]
pub struct HistoryRecord {
	pub player_a: String,
	pub player_b: String,
	pub deck_a: String,
	pub deck_b: String,
	pub player_c: Option<String>,
	pub player_d: Option<String>,
	pub deck_c: Option<String>,
	pub deck_d: Option<String>,
	pub winner_id: Option<u8>,
	pub room_id: String,
	pub first_attack_slot: u8,
	/// 本局真正开始前的消息边界，只在内存中使用，不写入数据库。
	pub replay_start: usize,
}

/// 使用上游记录的公开消息和 .yrp3d 序列化，但不以可重复发送的 STOC_DUEL_START 截取小局。
pub fn replay_buffer(duel: &ygopro::duel::Duel, history: &HistoryRecord) -> Result<Vec<u8>> {
	let recorded = duel.sender.masked_messages.get(history.replay_start..)
		.context("当前小局录像消息边界无效")?;
	let start = recorded.iter().position(|message| {
		matches!(message.try_get(), Ok(stoc::Message::GameMessage(game)) if matches!(&game.message, gm::Message::Start(start) if start.player_type & 0x10 != 0))
	}).context("当前小局缺少观战 START，无法保存录像")?;
	// 使用开局快照，玩家退出后不再依赖 duel.players 中是否还存在该玩家。
	// 上游双打名称顺序为 0 / 1 / 3 / 2，对应历史记录的 A / C / D / B。
	let tag = history.player_c.is_some();
	let client = history.player_d.as_deref().unwrap_or(&history.player_b);
	let names = gm::SibylName {
		host_name: history.player_a.as_str().into(),
		host_tag_name: history.player_c.as_deref().unwrap_or("").into(),
		host_current_name: history.player_a.as_str().into(),
		client_name: client.into(),
		client_tag_name: if tag { history.player_b.as_str() } else { "" }.into(),
		client_current_name: client.into(),
		master_rule: duel.host_info.duel_rule,
	};
	let mut replay = ygopro_data::data::forge::Replay { messages: vec![names.into()] };
	for (index, message) in recorded[start..].iter().enumerate() {
		let stoc::Message::GameMessage(game) = message.try_get().context("解析录像消息失败")? else { continue; };
		let message = &game.message;
		// 后续重连场面快照的 START 不能覆盖真正开局的观战 START。
		if (index != 0 && matches!(message, gm::Message::Start(_))) || message.waiting_for().is_some()
			|| matches!(message, gm::Message::SibylName(_) | gm::Message::SibylChat(_) | gm::Message::SibylReplay(_)) { continue; }
		replay.messages.push(message.clone());
	}
	let mut buffer = Cursor::new(Vec::new());
	replay.write_le(&mut buffer).context("序列化上游 .yrp3d 录像失败")?;
	Ok(buffer.into_inner())
}

pub async fn persist(record: HistoryRecord, buffer: Option<Vec<u8>>) -> Result<()> {
	let db = srvpro3_database::db().context("获取数据库连接失败")?;
	let replay = if let Some(buffer) = buffer {
		Some(tokio::task::spawn_blocking(move || -> Result<String> {
		let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
		gzip.write_all(&buffer).context("压缩录像失败")?;
		Ok(format!("{REPLAY_PREFIX}{}", STANDARD.encode(gzip.finish().context("完成录像压缩失败")?)))
		}).await.context("录像压缩任务失败")??)
	} else { None };
	let winner = match record.winner_id {
		Some(0) => Some(record.player_a.clone()),
		Some(1) => Some(record.player_b.clone()),
		_ => None,
	};
	srvpro3_database::history::create(
		&db, record.player_a, record.player_b, record.deck_a, record.deck_b,
		winner, record.room_id, replay,
		record.player_c, record.player_d, record.deck_c, record.deck_d,
	).await.context("保存小局历史记录失败")?;
	Ok(())
}
