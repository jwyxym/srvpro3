use std::{collections::BTreeMap, sync::Arc};
use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use parking_lot::RwLockReadGuard;
use srvpro3_cards::CardsSnapshot;
use srvpro3_config::Config;
use ygopro_cdb_reader::Card;

#[derive(Serialize)]
pub struct Counts {
	pub image_url: String,
	pub cards: usize,
	pub packs: usize,
	pub lflists: usize,
}

pub async fn get() -> Result<Json<Counts>, (StatusCode, &'static str)> {
	let snapshot: Arc<CardsSnapshot> = srvpro3_cards::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "卡片数据尚未加载"))?;
	Ok(Json(Counts {
		image_url: image_url()?,
		cards: snapshot.cards.len(),
		packs: snapshot.packs.len(),
		lflists: snapshot.lflists.len(),
	}))
}

fn image_url() -> Result<String, (StatusCode, &'static str)> {
	Ok(srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?
		.cards.image_url.trim().to_owned())
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
	#[default]
	All,
	Monster,
	Spell,
	Trap,
}

#[derive(Deserialize)]
pub struct SearchQuery {
	#[serde(default)]
	pub q: String,
	#[serde(default)]
	pub kind: Kind,
	#[serde(default)]
	pub page: usize,
	#[serde(default = "page_size")]
	pub page_size: usize,
}

fn page_size() -> usize { 20 }

#[derive(Serialize)]
pub struct CardResponse {
	code: u32,
	pack: String,
	source: String,
	image_url: String,
	name: String,
	desc: String,
	alias: u32,
	card_type: u32,
	level: u32,
	attribute: u32,
	race: u32,
	attack: i32,
	defense: i32,
	lscale: u32,
	rscale: u32,
	link_marker: u32,
}

impl From<&Card> for CardResponse {
	fn from(card: &Card) -> Self {
		Self {
			pack: String::new(), source: String::new(), image_url: String::new(),
			code: card.code, name: card.name.clone(), desc: card.desc.clone(), alias: card.alias,
			card_type: card.card_type, level: card.level, attribute: card.attribute, race: card.race,
			attack: card.attack, defense: card.defense, lscale: card.lscale, rscale: card.rscale,
			link_marker: card.link_marker,
		}
	}
}

#[derive(Serialize)]
pub struct SearchResponse {
	image_url: String,
	list: Vec<CardResponse>,
	total: usize,
}

// 与 expansions 使用相同的相对/绝对路径形式；兼容 Windows 分隔符、./ 和目录末尾斜线。
fn source_key(value: &str) -> String {
	let mut key: String = value.trim().replace('\\', "/");
	while key.starts_with("./") { key = key[2..].to_owned(); }
	if key == "." { return String::new(); }
	key.trim_end_matches('/').to_owned()
}

pub async fn search(Query(query): Query<SearchQuery>) -> Result<Json<SearchResponse>, (StatusCode, &'static str)> {
	if query.q.chars().count() > 100 || !(1..=100).contains(&query.page_size) {
		return Err((StatusCode::BAD_REQUEST, "关键词最多 100 个字符，每页数量须为 1 到 100"));
	}
	let offset: usize = query.page.checked_mul(query.page_size)
		.ok_or((StatusCode::BAD_REQUEST, "分页参数无效"))?;
	let snapshot: Arc<CardsSnapshot> = srvpro3_cards::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "卡片数据尚未加载"))?;
	let (image_url, pack_image_urls): (String, BTreeMap<String, String>) = {
		let config: RwLockReadGuard<'_, Config> = srvpro3_config::get()
			.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
		(config.cards.image_url.trim().to_owned(), config.cards.pack_image_urls.iter()
			.filter(|(_, url): &(&String, &String)| !url.trim().is_empty())
			.map(|(key, url): (&String, &String)| (source_key(key), url.trim().to_owned())).collect())
	};
	// 使用本次快照完成筛选和分页，卡库重载不会使同一次查询读到混合数据。
	let result: SearchResponse = tokio::task::spawn_blocking(move || {
		let keyword: String = query.q.trim().to_lowercase();
		let code: Option<u32> = keyword.parse().ok();
		let mask: u32 = match query.kind { Kind::All => 0, Kind::Monster => 1, Kind::Spell => 2, Kind::Trap => 4 };
		let mut matches: Vec<&Card> = snapshot.cards.values().filter(|card: &&Card| {
			(mask == 0 || card.card_type & mask != 0) && (keyword.is_empty()
				|| card.code.to_string().contains(&keyword)
				|| card.name.to_lowercase().contains(&keyword)
				|| card.desc.to_lowercase().contains(&keyword))
		}).collect();
		// 精确卡号优先，其余按卡号排序，避免 HashMap 的遍历顺序造成翻页重复。
		matches.sort_unstable_by_key(|card: &&Card| (Some(card.code) != code, card.code));
		let total: usize = matches.len();
		let list: Vec<CardResponse> = matches.into_iter().skip(offset).take(query.page_size).map(|card: &Card| {
			let mut response: CardResponse = CardResponse::from(card);
			response.source = snapshot.sources.get(&card.code).map(|path: &String| source_key(path)).unwrap_or_default();
			let (directory, filename): (&str, &str) = response.source.rsplit_once('/').unwrap_or(("", &response.source));
			response.pack = filename.to_owned();
			response.image_url = pack_image_urls.get(&response.source)
				.or_else(|| pack_image_urls.get(directory))
				.or_else(|| pack_image_urls.get(filename)).unwrap_or(&image_url).clone();
			response
		}).collect();
		SearchResponse { image_url, total, list }
	}).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "搜索卡片失败"))?;
	Ok(Json(result))
}
