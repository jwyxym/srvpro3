use anyhow::{Error, ensure, Context};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time::timeout};
use std::time::Duration;

use super::Bot;

pub async fn add(bot: Bot) -> Result<(), Error> {
	let config = srvpro3_config::get()?;
	let path: String = config.windbot.path.clone();
	let port: u16 = config.windbot.port;
	drop(config);
	ensure!(!path.trim().is_empty(), "WindBot 未启用");
	ensure!(port != 0, "WindBot 未启用");
	ensure!(!bot.name.is_empty(), "bot 名称不能为空");
	ensure!(!bot.host.is_empty(), "bot 目标地址不能为空");
	ensure!(bot.port != 0, "bot 目标端口不能为 0");
	let mut query: Vec<(&str, String)> = vec![
		("name", bot.name),
		("deck", bot.deck),
		("host", bot.host),
		("port", bot.port.to_string()),
		("password", bot.password),
		("debug", bot.debug.to_string()),
		("chat", bot.chat.to_string())
	];
	if let Some(dialog) = bot.dialog { query.push(("dialog", dialog)); }
	if let Some(version) = bot.version { query.push(("version", version.to_string())); }
	if let Some(hand) = bot.hand { query.push(("hand", hand.to_string())); }
	let query: String = query.into_iter().map(|(key, value)| format!("{key}={}", encode(&value))).collect::<Vec<_>>().join("&");
	let address: String = format!("127.0.0.1:{}", port);
	let mut stream: TcpStream = timeout(Duration::from_secs(10), TcpStream::connect(&address)).await
		.context("连接 WindBot server 超时")??;
	let request = format!("GET /?{query} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
	timeout(Duration::from_secs(10), stream.write_all(request.as_bytes())).await
		.context("向 WindBot server 发送添加 bot 请求超时")??;
	let mut response: Vec<u8> = Vec::new();
	timeout(Duration::from_secs(10), stream.read_to_end(&mut response)).await
		.context("等待 WindBot server 响应超时")??;
	let response: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&response);
	ensure!(response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200"), "WindBot 添加 bot 失败：{}", response.lines().next().unwrap_or("无响应"));
	Ok(())
}

fn encode(value: &str) -> String {
	let mut result: String = String::with_capacity(value.len());
	for byte in value.bytes() {
		if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
			result.push(byte as char);
		} else {
			use std::fmt::Write;
			let _ = write!(result, "%{byte:02X}");
		}
	}
	result
}
