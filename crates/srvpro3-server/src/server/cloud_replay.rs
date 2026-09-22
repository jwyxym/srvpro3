use std::{io::{Cursor, Read}, time::Duration};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use binrw::BinRead;
use flate2::read::GzDecoder;
use tokio::sync::Semaphore;
use ygopro_data::{constants::{Mode, Netplayer}, message::{HostInfo, ctos, gm::{self, GameMessage}, stoc}};
use super::{Server, decode::decode, reconnect, transport::{Connection, Protocol}};

const MAX_REPLAY: usize = 64 * 1024 * 1024;
static VIEWERS: Semaphore = Semaphore::const_new(32);

struct Replay {
	players: Vec<(u8, String)>,
	info: HostInfo,
	buffer: Vec<u8>,
}

async fn load(connection: &Connection) -> Result<Replay> {
	let id = connection.handshake.pass.strip_prefix("R#").unwrap_or_default();
	ensure!(!id.is_empty() && id.bytes().all(|value| value.is_ascii_digit()), "云录像编号必须为正整数");
	let id: i64 = id.parse().context("云录像编号超出范围")?;
	ensure!(id > 0, "云录像编号必须为正整数");
	ensure!(reconnect::version(connection) == Some(*ygopro::plugin::version_check::PRO_VERSION), "客户端协议版本不匹配");
	let record = tokio::time::timeout(Duration::from_secs(10), srvpro3_database::history::read::by_id(id))
		.await.context("查询云录像超时")??.context("未找到该历史记录")?;
	tokio::task::spawn_blocking(move || decode_record(record)).await.context("读取录像任务失败")?
}

fn decode_record(record: srvpro3_database::history::Model) -> Result<Replay> {
	let buffer = unpack(record.replay.as_deref().context("该历史记录没有录像")?)?;
	decode_forge(&buffer)
}

fn unpack(encoded: &str) -> Result<Vec<u8>> {
	let compressed = compressed_replay(encoded)?;
	let mut buffer = Vec::new();
	GzDecoder::new(compressed.as_slice()).take(MAX_REPLAY as u64 + 1)
		.read_to_end(&mut buffer).context("解压录像失败")?;
	ensure!(buffer.len() <= MAX_REPLAY, "录像解压后过大");
	Ok(buffer)
}

fn compressed_replay(encoded: &str) -> Result<Vec<u8>> {
	ensure!(encoded.len() <= MAX_REPLAY, "录像文件过大");
	let encoded = encoded.strip_prefix(super::history::REPLAY_PREFIX).context("不支持旧版观战帧，请下载新保存的 .yrp3d 录像")?;
	let compressed = STANDARD.decode(encoded).context("录像 Base64 格式无效")?;
	ensure!(compressed.starts_with(&[0x1f, 0x8b, 0x08]), "录像 gzip 格式无效");
	Ok(compressed)
}

/// 返回 gzip 压缩的 .yrp3d，由下载端解压；调用方应在阻塞线程中执行。
pub fn export(record: srvpro3_database::history::Model) -> Result<Vec<u8>> {
	compressed_replay(record.replay.as_deref().context("该历史记录没有录像")?)
}

fn parse_forge(buffer: &[u8]) -> Result<ygopro_data::data::forge::Replay> {
	// 先检查上游记录的长度字段，避免损坏数据让解析器按伪造长度分配内存。
	let mut offset = 0;
	while offset < buffer.len() {
		ensure!(buffer.len() - offset >= 5, ".yrp3d 记录头不完整");
		let length = u32::from_le_bytes(buffer[offset + 1..offset + 5].try_into().unwrap()) as usize;
		offset += 5;
		ensure!(length <= buffer.len() - offset, ".yrp3d 记录长度无效");
		offset += length;
	}
	let mut cursor = Cursor::new(buffer);
	let replay = ygopro_data::data::forge::Replay::read_le(&mut cursor).context("解析上游 .yrp3d 录像失败")?;
	ensure!(cursor.position() == buffer.len() as u64, ".yrp3d 存在多余数据");
	ensure!(matches!(replay.messages.first(), Some(gm::Message::SibylName(_))), ".yrp3d 缺少玩家信息");
	ensure!(matches!(replay.messages.get(1), Some(gm::Message::Start(start)) if start.player_type & 0x10 != 0), ".yrp3d 缺少观战 START");
	Ok(replay)
}

/// 云播放时临时生成网络消息，不存储观战帧。
fn decode_forge(buffer: &[u8]) -> Result<Replay> {
	let replay = parse_forge(buffer)?;
	let Some(gm::Message::SibylName(names)) = replay.messages.first() else { anyhow::bail!(".yrp3d 缺少玩家信息"); };
	let mut players = vec![(0, names.host_name.to_string())];
	let tag = !names.host_tag_name.to_string().is_empty() || !names.client_tag_name.to_string().is_empty();
	if tag {
		players.push((1, names.host_tag_name.to_string()));
		players.push((2, names.client_tag_name.to_string()));
		players.push((3, names.client_name.to_string()));
	} else {
		players.push((1, names.client_name.to_string()));
	}
	let Some(gm::Message::Start(start)) = replay.messages.get(1) else { anyhow::bail!("录像缺少 START"); };
	let info = HostInfo {
		lflist: 0, duel_rule: start.rule,
		start_lp: u32::try_from(start.player1_lp).context("录像初始生命值无效")?,
		mode: if tag { Mode::Tag } else { Mode::Single }, ..HostInfo::default()
	};
	let mut frames = Vec::new();
	for message in replay.messages.into_iter().skip(1) {
		ensure!(!matches!(message, gm::Message::SibylName(_) | gm::Message::SibylChat(_) | gm::Message::SibylReplay(_)), ".yrp3d 包含非观战消息");
		ensure!(message.waiting_for().is_none(), ".yrp3d 包含玩家操作提示");
		let frame = reconnect::bytes(message.into());
		let length = u16::try_from(frame.len()).context("录像消息长度超出协议限制")?;
		ensure!(frames.len() + 2 + frame.len() <= MAX_REPLAY, "录像消息总长度过大");
		frames.extend_from_slice(&length.to_le_bytes());
		frames.extend_from_slice(&frame);
	}
	Ok(Replay { players, info, buffer: frames })
}

// 等待发送空间时同时处理退出，避免补发期间堵塞客户端输入。
async fn send(connection: &mut Connection, frame: Vec<u8>) -> Result<bool> {
	loop {
		tokio::select! {
			permit = connection.outgoing.reserve() => {
				permit.context("录像连接已关闭")?.send(frame);
				return Ok(true);
			}
			incoming = connection.incoming.recv() => {
				let Some(incoming) = incoming else { return Ok(false); };
				if matches!(decode(&incoming), Some(ctos::Message::LeaveGame(_))) { return Ok(false); }
			}
		}
	}
}

async fn play(connection: &mut Connection, replay: Replay) -> Result<()> {
	let position = Netplayer::Observer(0);
	let mut opening: Vec<stoc::Message> = vec![
		stoc::JoinGame { info: replay.info }.into(),
		stoc::TypeChange { player: position, host: false }.into(),
	];
	for (slot, name) in &replay.players {
		opening.push(stoc::HsPlayerEnter { name: name.as_str().into(), pos: Netplayer::Player(*slot) }.into());
	}
	opening.push(stoc::DuelStart.into());
	for message in opening {
		if !send(connection, reconnect::bytes(message)).await? { return Ok(()); }
	}
	let mut offset = 0;
	while offset < replay.buffer.len() {
		let length = u16::from_le_bytes([replay.buffer[offset], replay.buffer[offset + 1]]) as usize;
		offset += 2;
		if !send(connection, replay.buffer[offset..offset + length].to_vec()).await? { return Ok(()); }
		offset += length;
	}
	drop(replay.buffer);
	// 录像只保存 GAME_MSG；全部帧之后单独补发结束通知。
	if !send(connection, reconnect::bytes(stoc::DuelEnd.into())).await? { return Ok(()); }
	// 网络发送结束不等于客户端播放结束，保留连接至玩家退出，避免截断动画。
	while let Some(frame) = connection.incoming.recv().await {
		if matches!(decode(&frame), Some(ctos::Message::LeaveGame(_))) { break; }
	}
	Ok(())
}

pub async fn run(mut connection: Connection) {
	let Ok(_permit) = VIEWERS.try_acquire() else {
		Server::reject_with_reason(connection, "云录像观看人数已满，请稍后重试");
		return;
	};
	let replay = match load(&connection).await {
		Ok(replay) => replay,
		Err(error) => { Server::reject_with_reason(connection, &format!("读取云录像失败：{error}")); return; }
	};
	let _ = play(&mut connection, replay).await;
	if connection.protocol == Protocol::Udp {
		let _ = connection.outgoing.send(reconnect::bytes(stoc::LeaveGame { pos: Netplayer::Observer(0) }.into())).await;
		tokio::time::sleep(Duration::from_millis(200)).await;
	}
	if let Some(close) = connection.close.take() { let _ = close.send(()); }
}
