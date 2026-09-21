use anyhow::{Context, Result};
use std::io::{Cursor, Write};
use base64::{Engine, engine::general_purpose::STANDARD};
use binrw::BinWrite;
use flate2::{Compression, write::GzEncoder};
use ygopro_data::message::gm::{self, GameMessage};

pub const REPLAY_PREFIX: &str = "yrp3d:v1:";

/// 每局开始时固定玩家和卡组，避免换备、断线及换座位影响历史记录。
#[derive(Clone, Debug)]
pub struct HistoryRecord {
	pub player_a: String,
	pub player_b: String,
	pub deck_a: String,
	pub deck_b: String,
	pub winner_id: Option<u8>,
	pub room_id: String,
	pub first_attack_slot: u8,
}

/// 使用上游的消息流录像生成器和 .yrp3d 序列化，仅保存当前小局的公开观战视角。
pub fn replay_buffer(duel: &ygopro::duel::Duel) -> Result<Vec<u8>> {
	let mut replay = duel.create_replay_forge(true).context("上游未生成当前小局录像")?;
	let start = replay.messages.iter().rposition(|message| {
		matches!(message, gm::Message::Start(start) if start.player_type & 0x10 != 0)
	}).context("当前小局缺少观战 START，无法保存录像")?;
	// 保留上游玩家信息；不保存操作提示、聊天或内嵌的完整卡组录像。
	let names = replay.messages.iter().find(|message| matches!(message, gm::Message::SibylName(_))).cloned();
	replay.messages.drain(..start);
	replay.messages.retain(|message| message.waiting_for().is_none()
		&& !matches!(message, gm::Message::SibylName(_) | gm::Message::SibylChat(_) | gm::Message::SibylReplay(_)));
	if let Some(names) = names { replay.messages.insert(0, names); }
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
	).await.context("保存小局历史记录失败")?;
	Ok(())
}
