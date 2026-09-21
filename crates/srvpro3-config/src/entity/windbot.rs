use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindBot {
	pub path: String,
	pub address: String,
	pub bots: String,
	pub port: u16
}

impl Default for WindBot {
	fn default() -> Self {
		Self {
			path: String::new(),
			address: "127.0.0.1".into(),
			bots: String::new(),
			port: 2399
		}
	}
}
