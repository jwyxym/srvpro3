use anyhow::{Context, Result};

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

pub async fn persist(records: Vec<HistoryRecord>) -> Result<()> {
	if records.is_empty() { return Ok(()); }
	let db = srvpro3_database::db().context("获取数据库连接失败")?;
	persist_to(&db, records).await
}

async fn persist_to(db: &sea_orm::DatabaseConnection, records: Vec<HistoryRecord>) -> Result<()> {
	let Some((record, winner)) = summarize(records) else { return Ok(()); };
	srvpro3_database::history::create(
		db, record.player_a, record.player_b, record.deck_a, record.deck_b,
		winner, record.room_id, None,
	).await.context("保存房间历史记录失败")?;
	Ok(())
}

// 房间只入库一次，卡组保留首局快照；按座位统计，不能按名字区分同名玩家。
fn summarize(records: Vec<HistoryRecord>) -> Option<(HistoryRecord, Option<String>)> {
	let mut wins = [0usize; 2];
	for record in &records {
		if let Some(slot @ 0..=1) = record.winner_id {
			wins[slot as usize] += 1;
		}
	}
	let record = records.into_iter().next()?;
	let winner = match wins[0].cmp(&wins[1]) {
		std::cmp::Ordering::Greater => Some(record.player_a.clone()),
		std::cmp::Ordering::Less => Some(record.player_b.clone()),
		std::cmp::Ordering::Equal => None,
	};
	Some((record, winner))
}
