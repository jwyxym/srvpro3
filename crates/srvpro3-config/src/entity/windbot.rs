use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindBot {
	pub address: String,
	pub bots: String,
	pub port: u16
}

impl Default for WindBot {
	fn default() -> Self {
		Self {
			address: "127.0.0.1".into(),
			bots: "windbot/bots.json".into(),
			port: 2399
		}
	}
}
