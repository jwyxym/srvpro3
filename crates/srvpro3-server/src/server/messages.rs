use std::{collections::BTreeMap, sync::{Arc, LazyLock}, time::Duration};
use anyhow::{Context, Result, ensure};
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use serde::Deserialize;
use tokio::{io::AsyncReadExt, sync::mpsc, time::Instant};
use ygopro::{duel::{Duel, SendTarget}, ygocore_handlers::{Handler, YGOCORE_HANDLERS}};
use ygopro_data::{constants::{Color, DuelStage, Netplayer}, message::{gm, stoc}};
use ygopro_derive::{after, register_to};
use super::{RoomEntry, reconnect::bytes};

pub const NAME: &str = module_path!();

#[derive(Clone, Deserialize)]
#[serde(default)]
struct TipsFile {
	prefix: String,
	interval: u64,
	tips: Vec<String>,
}

impl Default for TipsFile {
	fn default() -> Self {
		Self { prefix: String::new(), interval: 30, tips: Vec::new() }
	}
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct DialoguesFile {
	dialogues: BTreeMap<u32, Vec<String>>,
	dialogues_custom: BTreeMap<u32, Vec<String>>,
}

#[derive(Default)]
struct Resources {
	config: srvpro3_config::Server,
	tips: Arc<TipsFile>,
	dialogues: Arc<BTreeMap<u32, Vec<String>>>,
	tips_source: Option<Arc<[u8]>>,
	dialogue_files: Vec<DialogueResource>,
}

#[derive(Clone)]
struct DialogueResource {
	source: Arc<[u8]>,
	format: DialogueFormat,
	lines: Arc<BTreeMap<u32, Vec<String>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogueFormat { Json, Toml }

fn parse_dialogues(buffer: &[u8], format: DialogueFormat) -> Result<BTreeMap<u32, Vec<String>>> {
	let text = std::str::from_utf8(buffer)?.trim_start_matches('\u{feff}');
	let mut lines = match format {
		DialogueFormat::Toml => {
			// TOML 的键是字符串，显式转换并校验卡号。
			let values: BTreeMap<String, Vec<String>> = basic_toml::from_str(text)?;
			let mut lines = BTreeMap::new();
			for (key, value) in values {
				ensure!(!key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit()), "无效卡号：{key}");
				let code: u32 = key.parse().with_context(|| format!("卡号超出 u32 范围：{key}"))?;
				ensure!(lines.insert(code, value).is_none(), "重复卡号：{code}");
			}
			lines
		}
		DialogueFormat::Json => {
			let value: serde_json::Value = serde_json::from_str(text)?;
			if value.get("dialogues").is_some() || value.get("dialogues_custom").is_some() {
				// 兼容之前 srvpro2 的包装结构。
				let file: DialoguesFile = serde_json::from_value(value)?;
				let mut lines = file.dialogues_custom;
				for (code, mut values) in file.dialogues {
					values.retain(|line| !line.trim().is_empty());
					if !values.is_empty() { lines.insert(code, values); }
				}
				lines
			} else {
				serde_json::from_value::<BTreeMap<u32, Vec<String>>>(value)?
			}
		}
	};
	for values in lines.values_mut() { values.retain(|line| !line.trim().is_empty()); }
	Ok(lines)
}

static RESOURCES: LazyLock<RwLock<Arc<Resources>>> = LazyLock::new(|| RwLock::new(Arc::new(Resources::default())));

async fn read(path: &str) -> Result<Vec<u8>> {
	let file = tokio::fs::File::open(path).await.with_context(|| format!("打开 {path}"))?;
	let mut buffer = Vec::new();
	file.take(16 * 1024 * 1024 + 1).read_to_end(&mut buffer).await?;
	ensure!(buffer.len() <= 16 * 1024 * 1024, "消息资源文件超过 16 MiB");
	Ok(buffer)
}

async fn read_tips(path: &str, previous: Option<&[u8]>) -> Result<Option<(TipsFile, Arc<[u8]>)>> {
	let buffer = read(path).await?;
	if previous == Some(buffer.as_slice()) { return Ok(None); }
	let text = std::str::from_utf8(&buffer).with_context(|| format!("{path} 不是 UTF-8 文本"))?;
	let file = basic_toml::from_str(text).with_context(|| format!("解析 {path}"))?;
	Ok(Some((file, buffer.into())))
}

async fn read_dialogues(paths: &[String], previous: &[DialogueResource]) -> Result<Vec<DialogueResource>> {
	let mut files: Vec<DialogueResource> = Vec::new();
	for path in paths.iter().filter(|path| !path.trim().is_empty()) {
		let extension = std::path::Path::new(path).extension().and_then(|ext| ext.to_str()).unwrap_or_default();
		let format = match extension.to_ascii_lowercase().as_str() {
			"json" => DialogueFormat::Json,
			"toml" => DialogueFormat::Toml,
			_ => anyhow::bail!("台词文件只支持 .json 或 .toml：{path}"),
		};
		let buffer = read(path).await?;
		if let Some(cached) = previous.iter().chain(files.iter()).find(|file| file.format == format && file.source.as_ref() == buffer.as_slice()) {
			files.push(cached.clone());
			continue;
		}
		let lines = parse_dialogues(&buffer, format).with_context(|| format!("解析 {path}"))?;
		files.push(DialogueResource { source: buffer.into(), format, lines: Arc::new(lines) });
	}
	Ok(files)
}

pub async fn reload() -> Result<()> {
	let config = srvpro3_config::get()?.server.clone();
	let previous = RESOURCES.read().clone();
	let mut resources = Resources { config, ..Default::default() };
	if !resources.config.tips.trim().is_empty() {
		resources.tips = previous.tips.clone();
		resources.tips_source = previous.tips_source.clone();
		match read_tips(&resources.config.tips, previous.tips_source.as_deref()).await {
			Ok(Some((mut file, source))) => {
				file.tips.retain(|line| !line.trim().is_empty());
				resources.tips = Arc::new(file);
				resources.tips_source = Some(source);
				srvpro3_log::info!("Tips 加载成功");
			}
			Ok(None) => {}
			Err(error) => {
				srvpro3_log::warn!("Tips 加载失败：{error:#}");
			}
		}
		if resources.tips.tips.is_empty() {
			srvpro3_log::warn!("Tips 没有有效提示内容，无法发送提示");
		} else if resources.tips.interval == 0 {
			srvpro3_log::info!("Tips 定时提示已关闭（interval = 0），仍可使用 /tips");
		}
	} else {
		srvpro3_log::warn!("未启用Tips，如需启用，请修改config.toml的内容");
	}
	if resources.config.dialogues.iter().any(|path| !path.trim().is_empty()) {
		resources.dialogues = previous.dialogues.clone();
		resources.dialogue_files = previous.dialogue_files.clone();
		match read_dialogues(&resources.config.dialogues, &previous.dialogue_files).await {
			Ok(files) => {
				let unchanged = files.len() == previous.dialogue_files.len()
					&& files.iter().zip(&previous.dialogue_files).all(|(a, b)| a.format == b.format && a.source == b.source);
				if !unchanged {
					let mut dialogues: BTreeMap<u32, Vec<String>> = BTreeMap::new();
					for file in &files {
						for (code, lines) in file.lines.iter() {
							let merged = dialogues.entry(*code).or_default();
							for line in lines { if !merged.contains(line) { merged.push(line.clone()); } }
						}
					}
					resources.dialogues = Arc::new(dialogues);
				}
				resources.dialogue_files = files;
				if !unchanged {
					srvpro3_log::info!("Dialogues 加载成功：{} 个文件", resources.dialogue_files.len());
				}
			}
			Err(error) => {
				srvpro3_log::error!("Dialogues 加载失败：{error:#}");
			}
		}
		if resources.dialogues.values().all(Vec::is_empty) {
			srvpro3_log::warn!("Dialogues 没有有效台词内容，无法发送召唤台词");
		}
	} else {
		srvpro3_log::warn!("未启用Dialogues，如需启用，请修改config.toml的内容");
	}
	*RESOURCES.write() = Arc::new(resources);
	Ok(())
}

// 按 UTF-16 长度拆分聊天，给客户端固定长度缓冲区保留终止符空间。
fn chats(text: &str, color: Color) -> Vec<stoc::Message> {
	let mut result = Vec::new();
	for line in text.lines().filter(|line| !line.trim().is_empty()) {
		let mut chunk = String::new();
		let mut length = 0;
		for ch in line.chars() {
			if length + ch.len_utf16() > 240 {
				result.push(stoc::Chat { player: color.clone().into(), msg: std::mem::take(&mut chunk).into() }.into());
				if result.len() >= 16 { return result; }
				length = 0;
			}
			chunk.push(ch);
			length += ch.len_utf16();
		}
		if !chunk.is_empty() { result.push(stoc::Chat { player: color.clone().into(), msg: chunk.into() }.into()); }
		if result.len() >= 16 { break; }
	}
	result
}

pub fn welcome() -> Vec<Vec<u8>> {
	chats(&RESOURCES.read().config.welcome, Color::Green).into_iter().map(bytes).collect()
}

/// 通知已入房的其他连接，不把成员变动作为玩家聊天转交引擎。
pub fn membership(record: &super::room::RoomRecord, id: u64, joined: bool) {
	let Some(player) = record.players.get(&id) else { return };
	let role = match player.position {
		Netplayer::Player(_) => "玩家",
		Netplayer::Observer(_) => "观战者",
		_ => return,
	};
	let name: String = player.name.chars().filter(|ch| !ch.is_control()).collect();
	let action = if joined { "加入了" } else { "离开了" };
	let frames: Vec<_> = chats(&format!("{role}「{name}」{action}房间。"), Color::Lightblue)
		.into_iter().map(bytes).collect();
	for (&other_id, other) in &record.players {
		if other_id == id || !other.connected || !matches!(other.position, Netplayer::Player(_) | Netplayer::Observer(_)) { continue; }
		for frame in &frames {
			if other.outgoing.try_send(frame.clone()).is_err() { break; }
		}
	}
}

fn tip(resources: &Resources) -> Option<String> {
	if resources.config.tips.trim().is_empty() { return None; }
	resources.tips.tips.choose(&mut rand::thread_rng()).map(|line| format!("{}{line}", resources.tips.prefix))
}

fn send(outgoing: &mpsc::Sender<Vec<u8>>, text: &str, color: Color) {
	for message in chats(text, color) {
		if outgoing.try_send(bytes(message)).is_err() { break; }
	}
}

/// 在转交引擎和记录普通聊天之前处理，只回复发起连接，不触碰定时器。
pub fn command(text: &str, outgoing: &mpsc::Sender<Vec<u8>>) -> bool {
	if text.trim() != "/tips" { return false; }
	let resources = RESOURCES.read().clone();
	let message = tip(&resources).unwrap_or_else(|| {
		if resources.config.tips.trim().is_empty() { "提示功能未启用。".into() } else { "暂无可用提示。".into() }
	});
	send(outgoing, &message, Color::Lightblue);
	true
}

pub struct TipSchedule { last: Instant }

impl TipSchedule {
	pub fn new() -> Self { Self { last: Instant::now() } }

	pub fn tick(&mut self, rooms: &BTreeMap<String, RoomEntry>) {
		let resources = RESOURCES.read().clone();
		let now = Instant::now();
		if resources.config.tips.trim().is_empty() || resources.tips.interval == 0 {
			self.last = now;
			return;
		}
		if now.duration_since(self.last) < Duration::from_secs(resources.tips.interval) { return; }
		self.last = now;
		for room in rooms.values() {
			let record = room.record.lock().unwrap();
			if record.stage == DuelStage::End { continue; }
			let Some(message) = tip(&resources) else { continue; };
			for player in record.players.values() {
				if player.connected && matches!(player.position, Netplayer::Player(_) | Netplayer::Observer(_)) {
					send(&player.outgoing, &message, Color::Lightblue);
				}
			}
		}
	}
}

fn dialogue(duel: &mut Duel, code: u32) {
	let resources = RESOURCES.read().clone();
	if resources.config.dialogues.iter().all(|path| path.trim().is_empty()) { return; }
	let Some(line) = resources.dialogues.get(&code).and_then(|lines| lines.choose(&mut rand::thread_rng())) else { return; };
	for message in chats(line, Color::Pink) { duel.sender.send(message, SendTarget::All); }
}

#[after(gm::Summoning)]
#[register_to(YGOCORE_HANDLERS)]
fn summoned(duel: &mut Duel, message: &gm::Summoning) {
	if !message.position.position.is_face_down() { dialogue(duel, message.code); }
}

#[after(gm::SpecialSummoning)]
#[register_to(YGOCORE_HANDLERS)]
fn special_summoned(duel: &mut Duel, message: &gm::SpecialSummoning) {
	if !message.position.position.is_face_down() { dialogue(duel, message.code); }
}

#[after(gm::FlipSummoning)]
#[register_to(YGOCORE_HANDLERS)]
fn flip_summoned(duel: &mut Duel, message: &gm::FlipSummoning) { dialogue(duel, message.code); }
