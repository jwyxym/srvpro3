use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Server {
	pub tcp: Protocol,
	pub udp: Protocol,
	pub ws: WebSocket,
	/// 断线保留座位的秒数，0 禁用重连。
	pub reconnect_timeout_secs: u64,
	/// 换副限时秒数，0 禁用。
	pub side_timeout_secs: u64,
}

impl Default for Server {
	fn default() -> Self {
		Self {
			tcp: Protocol::default(),
			udp: Protocol::default(),
			ws: WebSocket::default(),
			reconnect_timeout_secs: 180,
			side_timeout_secs: 180,
		}
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Protocol {
	pub port: u16,
}

impl Default for Protocol {
	fn default() -> Self {
		Self { port: 7911 }
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WebSocket {
	pub port: u16,
}

impl Default for WebSocket {
	fn default() -> Self {
		Self { port: 7912 }
	}
}
