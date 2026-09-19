use super::decode::decode;
use anyhow::{Result, anyhow, bail};
use futures::StreamExt;
use ygopro_data::message::ctos::Message;

pub struct Handshake {
	pub name: String,
	pub pass: String,
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
		let message = decode(&frame).ok_or_else(|| match frame.first() {
			Some(opcode) => anyhow!("无效或不支持的握手消息：类型 0x{opcode:02X}，消息体 {} 字节", frame.len()),
			None => anyhow!("握手消息体为空"),
		})?;
		match &message {
			Message::PlayerInfo(value) => name = Some(value.name.to_string()),
			Message::JoinGame(value) => pass = Some(value.pass.to_string()),
			_ => {}
		}
		messages.push(message);
		if let (Some(name), Some(pass)) = (&name, &pass) {
			return Ok((Handshake { name: name.clone(), pass: pass.clone() }, messages));
		}
	}
	bail!("握手缺少 PlayerInfo 或 JoinGame")
}
