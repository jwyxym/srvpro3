use super::decode::decode;
use anyhow::{Result, anyhow, bail};
use futures::StreamExt;
use ygopro_data::message::ctos::Message;

pub struct Handshake {
	pub name: String,
	pub pass: String,
}

/// 从握手帧里按 UTF-16LE 解码字符串字段。
///
/// 客户端可能把定长字段正好填满而不带终止符，依赖的 `FixedLengthString` 在这种情况下
/// 解码失败，`Display` 会退化成固定的 "[ERROR]"，导致名字变成 "[ERROR]"、房间密码
/// 里解析不出 `#` 前缀而丢掉全部房间选项。这里直接读原始字节，读不到再回退。
fn decode_text(frame: &[u8], offset: usize) -> Option<String> {
	let bytes: &[u8] = frame.get(offset..)?;
	let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
	let length: usize = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
	if length == 0 { return None; }
	Some(String::from_utf16_lossy(&units[..length]))
}

pub async fn handshake(
	input: &mut super::transport::Input,
) -> Result<(Handshake, Vec<Message>)> {
	let mut messages = Vec::new();
	let mut name = None;
	let mut pass = None;

	// 收齐 PlayerInfo 和 JoinGame 即完成握手，不固定等待第三条消息。
	for _ in 0..8 {
		let frame = input.next().await.ok_or_else(|| anyhow!("握手连接已关闭"))??;
		// YGOPro3 会先发送 ExternalAddress 扩展包，当前引擎没有对应消息类型。
		// 此包只是客户端提供的地址信息，不参与身份判断，也不转交给引擎。
		if frame.first() == Some(&0x17) {
			continue;
		}
		let mut message = decode(&frame).ok_or_else(|| match frame.first() {
			Some(opcode) => anyhow!("无效或不支持的握手消息：类型 0x{opcode:02X}，消息体 {} 字节", frame.len()),
			None => anyhow!("握手消息体为空"),
		})?;
		match &mut message {
			Message::PlayerInfo(value) => {
				// 帧布局：类型 1 字节 + 名字 20 个 UTF-16 单元。
				let raw: String = decode_text(&frame, 1).unwrap_or_else(|| value.name.to_string());
				// $ 后面的扩展信息暂不使用；记录与转交引擎的名字必须保持一致。
				let display = raw.split_once('$').map_or(raw.as_str(), |(name, _)| name);
				if display.is_empty() { bail!("玩家名称不能为空"); }
				value.name = display.into();
				name = Some(display.to_owned());
			}
			Message::JoinGame(value) => {
				// 帧布局：类型 1 字节 + 版本 u16 + 填充 u16 + 房间号 u32 + 密码 20 个 UTF-16 单元。
				pass = Some(decode_text(&frame, 9).unwrap_or_else(|| value.pass.to_string()));
			}
			_ => {}
		}
		messages.push(message);
		if let (Some(name), Some(pass)) = (&name, &pass) {
			return Ok((Handshake { name: name.clone(), pass: pass.clone() }, messages));
		}
	}
	bail!("握手缺少 PlayerInfo 或 JoinGame")
}
