use std::{io::{Cursor, Read}, time::Duration};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use binrw::BinRead;
use flate2::read::GzDecoder;
use tokio::sync::Semaphore;
use ygopro_data::{constants::Netplayer, message::{HostInfo, ctos, gm, stoc}};
use super::{Server, decode::decode, reconnect, transport::{Connection, Protocol}};

const MAX_REPLAY: usize = 64 * 1024 * 1024;
static VIEWERS: Semaphore = Semaphore::const_new(32);

struct Replay {
	player_a: String,
	player_b: String,
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
	let encoded = record.replay.context("该历史记录没有录像")?;
	tokio::task::spawn_blocking(move || {
		ensure!(encoded.len() <= MAX_REPLAY, "录像文件过大");
		let compressed = STANDARD.decode(encoded).context("录像 Base64 格式无效")?;
		let mut buffer = Vec::new();
		GzDecoder::new(compressed.as_slice()).take(MAX_REPLAY as u64 + 1)
			.read_to_end(&mut buffer).context("解压录像失败")?;
		ensure!(buffer.len() <= MAX_REPLAY, "录像解压后过大");
		let mut offset = 0;
		let mut info = None;
		while offset < buffer.len() {
			ensure!(buffer.len() - offset >= 2, "录像消息长度头不完整");
			let length = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]) as usize;
			offset += 2;
			ensure!(length > 0 && length <= buffer.len() - offset, "录像消息长度无效");
			let mut cursor = Cursor::new(&buffer[offset..offset + length]);
			let message = stoc::Message::read_le(&mut cursor).context("录像消息解析失败")?;
			ensure!(cursor.position() == length as u64, "录像消息存在多余数据");
			let stoc::Message::GameMessage(game) = message else { anyhow::bail!("录像包含非游戏消息"); };
			if info.is_none() {
				let gm::Message::Start(start) = game.message else { anyhow::bail!("录像缺少 START 消息"); };
				ensure!(start.player_type & 0x10 != 0, "录像不是观战视角");
				let start_lp = u32::try_from(start.player1_lp).context("录像初始生命值无效")?;
				info = Some(HostInfo { lflist: 0, duel_rule: start.rule, start_lp, ..HostInfo::default() });
			}
			offset += length;
		}
		Ok(Replay { player_a: record.player_a, player_b: record.player_b, info: info.context("录像内容为空")?, buffer })
	}).await.context("读取录像任务失败")?
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
	let opening: Vec<stoc::Message> = vec![
		stoc::JoinGame { info: replay.info }.into(),
		stoc::TypeChange { player: position, host: false }.into(),
		stoc::HsPlayerEnter { name: replay.player_a.as_str().into(), pos: Netplayer::Player(0) }.into(),
		stoc::HsPlayerEnter { name: replay.player_b.as_str().into(), pos: Netplayer::Player(1) }.into(),
		stoc::DuelStart.into(),
	];
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
