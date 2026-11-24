use anyhow::Result;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::{Duration, Instant}};
use srvpro3_plugin::{Deck as PluginDeck, deck::Classification};
use srvpro3_deck::Deck;
use super::entity::{Column, Entity};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum RoomType {
	#[serde(alias = "single", alias = "SINGLE")]
	Single,
	#[serde(alias = "match", alias = "MATCH")]
	Match,
	#[serde(alias = "tag", alias = "TAG")]
	Tag,
}

#[derive(FromQueryResult)]
struct Record {
	id: i64,
	room_id: String,
	room_type: Option<String>,
	deck_a: String,
	deck_b: String,
	deck_c: Option<String>,
	deck_d: Option<String>,
}

impl Record {
	fn kind(&self) -> Option<RoomType> {
		match self.room_type.as_deref() {
			Some("Single") => return Some(RoomType::Single),
			Some("Match") => return Some(RoomType::Match),
			Some("Tag") => return Some(RoomType::Tag),
			_ => {}
		}
		if self.deck_c.is_some() || self.deck_d.is_some() { return Some(RoomType::Tag); }
		let (prefix, _): (&str, &str) = self.room_id.rsplit_once('#')?;
		// 与房间参数解析一致：先确定 M/MATCH 默认 BO，再让显式 BO 参数覆盖。
		let mut kind: Option<RoomType> = prefix.split([',', '，'])
			.any(|token: &str| matches!(token.trim().to_uppercase().as_str(), "M" | "MATCH"))
			.then_some(RoomType::Match);
		for token in prefix.split([',', '，']) {
			let token: String = token.trim().to_uppercase();
			if matches!(token.as_str(), "T" | "TAG") { return Some(RoomType::Tag); }
			if token == "SINGLE" { kind = Some(RoomType::Single); }
			if let Some(count) = token.strip_prefix("BO").and_then(|value: &str| value.parse::<u32>().ok()) {
				kind = Some(if count > 1 { RoomType::Match } else { RoomType::Single });
			}
		}
		// 其他规则前缀无法确定服务端当时的默认 BO，不推断为 Single。
		kind
	}
}

#[derive(Default, Serialize)]
pub struct Totals {
	pub usage_distribution: Vec<Bucket>,
	pub archetypes: Vec<Bucket>,
	pub classification_enabled: bool,
	pub total_duels: u64,
	pub total_decks: u64,
	pub used_decks: u64,
	pub copies: u64,
	pub skipped_decks: u64,
	/// 时间范围内无法识别类型的历史小局数；选择具体类型时不计入分母。
	pub unknown_type_duels: u64,
	pub usage_rate: f64,
}

#[derive(Serialize)]
pub struct Bucket { pub id: String, pub name: String, pub value: u64 }

pub async fn calculate(id: Option<u32>, room_type: Option<RoomType>, start: i64, end: i64) -> Result<Totals> {
	let db: DatabaseConnection = crate::db()?;
	let mut totals: Totals = Totals::default();
	let path: String = srvpro3_config::get()?.cards.wasm.trim().to_owned();
	totals.classification_enabled = !path.is_empty();
	let deadline: Instant = Instant::now() + Duration::from_secs(30);
	let mut classifier: Option<PluginDeck> = if path.is_empty() { None } else {
		Some(tokio::task::spawn_blocking(move || PluginDeck::load(&path)).await??)
	};
	let mut archetypes: BTreeMap<String, Bucket> = BTreeMap::new();
	let mut cursor: i64 = 0;
	// 固定查询上界，不让统计过程中追加的历史记录无限延长扫描。
	let last: Option<Record> = Entity::find().select_only()
		.columns([Column::Id, Column::RoomId, Column::RoomType, Column::DeckA, Column::DeckB, Column::DeckC, Column::DeckD])
		.filter(Column::CreatedAt.gte(start)).filter(Column::CreatedAt.lt(end))
		.order_by_desc(Column::Id).into_model::<Record>().one(&db).await?;
	let Some(last) = last else { return Ok(totals); };
	loop {
		// 不读取录像正文，按主键分批处理，避免一次加载整月的录像及卡组。
		let records: Vec<Record> = Entity::find().select_only()
			.columns([Column::Id, Column::RoomId, Column::RoomType, Column::DeckA, Column::DeckB, Column::DeckC, Column::DeckD])
			.filter(Column::CreatedAt.gte(start)).filter(Column::CreatedAt.lt(end))
			.filter(Column::Id.gt(cursor)).filter(Column::Id.lte(last.id))
			.order_by_asc(Column::Id).limit(500).into_model::<Record>().all(&db).await?;
		if records.is_empty() { break; }
		let mut classify_decks: Vec<Deck> = Vec::new();
		for record in records {
			cursor = record.id;
			let kind: Option<RoomType> = record.kind();
			if kind.is_none() { totals.unknown_type_duels += 1; }
			if room_type.is_some() && kind != room_type { continue; }
			totals.total_duels += 1;
			for deck in [Some(record.deck_a.as_str()), Some(record.deck_b.as_str()), record.deck_c.as_deref(), record.deck_d.as_deref()].into_iter().flatten() {
				if deck.is_empty() { totals.skipped_decks += 1; continue; }
				match srvpro3_deck::decode(deck) {
					Ok(deck) => {
						let deck: Deck = deck;
						let count: u32 = id.map_or(0, |code: u32| deck.count(code));
						totals.total_decks += 1;
						totals.copies += u64::from(count);
						if count > 0 { totals.used_decks += 1; }
						if id.is_none() || count > 0 {
							classify_decks.push(deck);
						}
					}
					Err(_) => totals.skipped_decks += 1,
				}
			}
		}
		if !classify_decks.is_empty() {
			let classified: (Option<PluginDeck>, BTreeMap<String, Bucket>) = tokio::task::spawn_blocking(move || -> Result<(Option<PluginDeck>, BTreeMap<String, Bucket>)> {
				for deck in classify_decks {
					anyhow::ensure!(Instant::now() < deadline, "卡组分类统计超时");
					let class: Classification = if let Some(classifier) = classifier.as_mut() { classifier.classify(&deck)? }
						else { Classification { id: "unclassified".into(), name: "未分类".into() } };
					let bucket: &mut Bucket = archetypes.entry(class.id.clone()).or_insert(Bucket { id: class.id, name: class.name.clone(), value: 0 });
					anyhow::ensure!(bucket.name == class.name, "WASM 同一标目标识返回了不同名称");
					bucket.value += 1;
				}
				Ok((classifier, archetypes))
			}).await??;
			classifier = classified.0;
			archetypes = classified.1;
		}
	}
	if totals.total_decks > 0 { totals.usage_rate = totals.used_decks as f64 / totals.total_decks as f64 * 100.0; }
	if id.is_some() {
		totals.usage_distribution = vec![
			Bucket { id: "used".into(), name: "使用该卡".into(), value: totals.used_decks },
			Bucket { id: "unused".into(), name: "未使用该卡".into(), value: totals.total_decks - totals.used_decks },
		];
	}
	totals.archetypes = archetypes.into_values().collect();
	totals.archetypes.sort_by(|left: &Bucket, right: &Bucket| right.value.cmp(&left.value).then(left.id.cmp(&right.id)));
	Ok(totals)
}
