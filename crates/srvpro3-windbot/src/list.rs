use std::{
	hash::{BuildHasher, Hash, Hasher, RandomState},
	sync::atomic::{AtomicU64, Ordering},
	time::{SystemTime, UNIX_EPOCH}
};
use anyhow::{Context, Error, ensure};
use serde::Deserialize;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize)]
pub struct BotInfo {
	pub name: String,
	#[serde(rename = "deck", alias = "ai_name")]
	pub ai_name: String,
	pub dialog: String,
}

#[derive(Deserialize)]
struct BotFile {
	windbots: Vec<BotInfo>,
}

/// 仅从配置的 bots.json 读取机器人列表；路径留空时使用默认文件。
pub fn list() -> Result<Vec<BotInfo>, Error> {
	let config = srvpro3_config::get()?;
	let bots_path = config.windbot.bots.clone();
	drop(config);
	let bots_path = if bots_path.trim().is_empty() { "windbot/bots.json" } else { &bots_path };
	let json = std::fs::read_to_string(bots_path)
		.with_context(|| format!("读取 WindBot 列表文件失败：{bots_path}"))?;
	let file: BotFile = serde_json::from_str(json.trim_start_matches('\u{feff}'))
		.with_context(|| format!("解析 WindBot 列表文件失败：{bots_path}"))?;
	ensure!(file.windbots.iter().all(|bot| !bot.name.trim().is_empty() && !bot.ai_name.trim().is_empty()), "WindBot 列表中的 name 和 deck 不能为空");
	Ok(file.windbots)
}

/// 随机选择一个非 Lucky 的 WindBot。
pub fn random() -> Result<BotInfo, Error> {
	let bots: Vec<BotInfo> = list()?.into_iter()
		.filter(|bot| bot.ai_name != "Lucky")
		.collect();
	ensure!(!bots.is_empty(), "WindBot 没有可用的随机 Bot");
	Ok(random_from(bots))
}

/// 按完整名称、AIName 或名称中连字符分隔的部分匹配，并从结果中随机选择。
pub fn select(name: &str) -> Result<BotInfo, Error> {
	let name = name.trim();
	if name.is_empty() { return random(); }
	let bots = list()?.into_iter().filter(|bot| {
		bot.name.trim() == name || bot.ai_name.eq_ignore_ascii_case(name)
			|| bot.name.split('-').any(|part| part.trim() == name)
	}).collect::<Vec<_>>();
	ensure!(!bots.is_empty(), "未找到机器人：{name}");
	Ok(random_from(bots))
}

fn random_from(mut bots: Vec<BotInfo>) -> BotInfo {
	let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
	let mut hasher = RandomState::new().build_hasher();
	time.hash(&mut hasher);
	NEXT.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
	let index = (hasher.finish() as usize) % bots.len();
	bots.swap_remove(index)
}
