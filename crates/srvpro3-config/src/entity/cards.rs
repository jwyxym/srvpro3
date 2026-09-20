use serde::{
	Deserialize, Deserializer, Serialize, de::{self, Visitor}
};
use std::{collections::BTreeMap, fmt};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Cards {
	pub expansions: Vec<String>,
	pub ypk: bool,
	pub lflist: LFlist,
	#[serde(with = "excode")]
	pub excode: BTreeMap<u32, Vec<u16>>
}

impl Default for Cards {
	fn default() -> Self {
		Self {
			expansions: Vec::new(),
			ypk: true,
			lflist: LFlist::None,
			excode: BTreeMap::from([
				(8512558, vec![0x8f, 0x54, 0x59, 0x82, 0x13a]),
				(55088578, vec![0x8f, 0x54, 0x59, 0x82, 0x13a])
			])
		}
	}
}

mod excode {
	use std::{collections::BTreeMap, fmt};
	use serde::{Deserializer, Serializer, de::{Error, MapAccess, Visitor}, ser::SerializeMap};

	pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<u32, Vec<u16>>, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct ExcodeVisitor;

		impl<'de> Visitor<'de> for ExcodeVisitor {
			type Value = BTreeMap<u32, Vec<u16>>;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("以卡片 ID 为键、setcode 列表为值的表")
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: MapAccess<'de>,
			{
				let mut excode = BTreeMap::new();
				while let Some((code, setcodes)) = map.next_entry::<String, Vec<u16>>()? {
					let code = code.parse().map_err(A::Error::custom)?;
					excode.insert(code, setcodes);
				}
				Ok(excode)
			}
		}

		deserializer.deserialize_map(ExcodeVisitor)
	}

	pub fn serialize<S>(value: &BTreeMap<u32, Vec<u16>>, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut map = serializer.serialize_map(Some(value.len()))?;
		for (code, setcodes) in value {
			map.serialize_entry(&code.to_string(), setcodes)?;
		}
		map.end()
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

