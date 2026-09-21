use serde::{Deserialize, Serialize};
use ygopro_data::constants::Rule;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Server {
	pub repaly: bool,
	pub tcp: Protocol,
	pub udp: Protocol,
	pub ws: WebSocket,
	pub bo: u8,
	pub lflist: i64,
	#[serde(with = "rule")]
	pub ot: Rule,
	pub shuffle: bool,
	pub master_rule: u8,
	pub draw_count: u8,
	pub start_hand: u8,
	pub time_limit: u16,
	pub start_lp: u32,
	pub reconnect_timeout: u64,
	pub side_timeout: u64
}

impl Default for Server {
	fn default() -> Self {
		Self {
			repaly: true,
			tcp: Protocol::default(),
			udp: Protocol::default(),
			ws: WebSocket::default(),
			bo: 1,
			lflist: -1,
			ot: Rule::OCG,
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

mod rule {
	use serde::{Deserialize, Deserializer, Serializer};
	use ygopro_data::constants::Rule;

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Rule, D::Error>
	where
		D: Deserializer<'de>,
	{
		let value: u8 = u8::deserialize(deserializer)?;
		Ok(Rule::try_from(value).unwrap_or(Rule::All))
	}

	pub fn serialize<S>(value: &Rule, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_u8(*value as u8)
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
