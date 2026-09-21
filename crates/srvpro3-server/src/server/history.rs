use anyhow::{Context, Result};
use std::io::Write;
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::GzEncoder};

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

/// 每帧为 u16 小端长度 + STOC 消息体，与传输层帧格式一致。
pub fn replay_buffer(duel: &ygopro::duel::Duel) -> Result<Vec<u8>> {
	let mut buffer = Vec::new();
	for message in super::spectate::messages(duel) {
		let size = u16::try_from(message.data.len()).context("录像消息长度超出协议限制")?;
		buffer.extend_from_slice(&size.to_le_bytes());
		buffer.extend_from_slice(&message.data);
	}
	anyhow::ensure!(!buffer.is_empty(), "当前小局缺少观战 START，无法保存录像");
	Ok(buffer)
}

pub async fn persist(record: HistoryRecord, buffer: Option<Vec<u8>>) -> Result<()> {
	let db = srvpro3_database::db().context("获取数据库连接失败")?;
	let replay = if let Some(buffer) = buffer {
		Some(tokio::task::spawn_blocking(move || -> Result<String> {
		let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
		gzip.write_all(&buffer).context("压缩录像失败")?;
		Ok(STANDARD.encode(gzip.finish().context("完成录像压缩失败")?))
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
