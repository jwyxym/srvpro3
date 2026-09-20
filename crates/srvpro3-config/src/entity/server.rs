use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Server {
	pub tcp: Protocol,
	pub udp: Protocol,
	pub ws: WebSocket,
	pub bo: u8,
	pub lflist: i64,
	pub shuffle: bool,
	pub master_rule: u8,
	pub draw_count: u8,
	pub start_hand: u8,
	pub time_limit: u16,
	pub start_lp: u32,
	#[serde(alias = "reconnect_timeout_secs")]
	pub reconnect_timeout: u64,
	#[serde(alias = "side_timeout_secs")]
	pub side_timeout: u64
}

impl Default for Server {
	fn default() -> Self {
		Self {
			tcp: Protocol::default(),
			udp: Protocol::default(),
			ws: WebSocket::default(),
			bo: 1,
			lflist: -1,
			shuffle: true,
			master_rule: 5,
			draw_count: 1,
			start_hand: 5,
			time_limit: 180,
			start_lp: 8000,
			reconnect_timeout: 180,
			side_timeout: 180,
		}
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Protocol {
	pub port: u16
}

impl Default for Protocol {
	fn default() -> Self {
		Self { port: 7911 }
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WebSocket {
	pub port: u16
}

impl Default for WebSocket {
	fn default() -> Self {
		Self { port: 7912 }
	}
}
