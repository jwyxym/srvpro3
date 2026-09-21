use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Cards {
	pub expansions: Vec<String>,
	pub ypk: bool,
	/// 自动重载间隔，单位秒；小于等于 0 表示禁用。
	pub reload: i32,
	#[serde(with = "excode")]
	pub excode: BTreeMap<u32, Vec<u16>>
}

impl Default for Cards {
	fn default() -> Self {
		Self {
			expansions: Vec::new(),
			ypk: true,
			reload: 1800,
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

