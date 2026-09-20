use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindBot {
	pub path: String,
	pub port: u16
}

impl Default for WindBot {
	fn default() -> Self {
		Self {
			path: String::new(),
			port: 2399
		}
	}
}