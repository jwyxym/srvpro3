use std::{
	ffi::{CStr, c_char},
	hash::{BuildHasher, Hash, Hasher, RandomState},
	path::PathBuf,
	sync::atomic::{AtomicU64, Ordering},
	time::{SystemTime, UNIX_EPOCH}
};
use anyhow::{Context, Error, ensure};
use libloading::Library;

type List = unsafe extern "C" fn() -> *mut c_char;
type Free = unsafe extern "C" fn(*mut c_char);

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct BotInfo {
	pub name: String,
	pub ai_name: String,
	pub dialog: String,
}

fn library_path(path: &str) -> PathBuf {
	let path = PathBuf::from(path);
	#[cfg(target_os = "windows")]
	let path = path.join("WindBot.dll");
	#[cfg(target_os = "linux")]
	let path = path.join("WindBot.so");
	#[cfg(target_os = "macos")]
	let path = path.join("WindBot.dylib");
	path
}

/// 从 WindBot 动态库读取当前已加载的 Bot 列表。
pub fn list() -> Result<Vec<BotInfo>, Error> {
	let config = srvpro3_config::get()?;
	let library_path = library_path(&config.windbot.path);
	drop(config);
	let library = unsafe { Library::new(&library_path) }
		.with_context(|| format!("加载 WindBot 动态库失败：{}", library_path.display()))?;
	let list: List = *unsafe { library.get::<List>(b"windbot_list\0") }
		.context("WindBot 动态库缺少 windbot_list")?;
	let free: Free = *unsafe { library.get::<Free>(b"windbot_free\0") }
		.context("WindBot 动态库缺少 windbot_free")?;
	let pointer = unsafe { list() };
	ensure!(!pointer.is_null(), "获取 WindBot Bot 列表失败");
	let json = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
	unsafe { free(pointer); }
	let values: Vec<[String; 3]> = serde_json::from_str(&json)
		.context("解析 WindBot Bot 列表失败")?;
	Ok(values.into_iter().map(|[name, ai_name, dialog]| BotInfo { name, ai_name, dialog }).collect())
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
