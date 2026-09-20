use serde::{
	Deserialize, Deserializer, Serialize, de::{self, Visitor}
};
use std::fmt;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Cards {
	pub expansions: Vec<String>,
	pub ypk: bool,
	pub lflist: LFlist
}

impl Default for Cards {
	fn default() -> Self {
		Self {
			expansions: Vec::new(),
			ypk: true,
			lflist: LFlist::None
		}
	}
}

#[derive(Serialize, Clone, Debug, Default)]
pub enum LFlist {
	Name(String),
	Index(i64),
	#[default]
	None
}

impl<'de> Deserialize<'de> for LFlist {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct LFlistVisitor;

		impl<'de> Visitor<'de> for LFlistVisitor {
			type Value = LFlist;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("")
			}

			fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E>  {
				Ok(if value < 0 {
					LFlist::None
				} else {
					LFlist::Index(value)
				})
			}

			fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E>  {
				Ok(if value.trim().is_empty() {
					LFlist::None
				} else {
					LFlist::Name(value.to_string())
				})
			}
		}

		deserializer.deserialize_any(LFlistVisitor)
	}
}

