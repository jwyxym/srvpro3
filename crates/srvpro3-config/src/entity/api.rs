use serde::{
	de::{self, Visitor},
	Deserialize, Deserializer, Serialize,
};
use std::{fmt, collections::BTreeMap};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct HttpApi {
	pub port: u16,
	pub room: bool,
	pub history: bool,
	pub user: BTreeMap<String, User>
}

impl Default for HttpApi {
	fn default() -> Self {
		Self {
			port: 8080,
			room: true,
			history: true,
			user: BTreeMap::from([("user".into(), User::default())]),
		}
	}
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct User {
	pub password: String,
	pub permissions: Permissions
}

#[derive(Serialize, Clone, Debug, Default)]
pub enum Permissions {
	Sudo = 3,
	Read = 2,
	Write = 1,
	#[default]
	None = 0
}

impl<'de> Deserialize<'de> for Permissions {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct PermissionsVisitor;

		impl<'de> Visitor<'de> for PermissionsVisitor {
			type Value = Permissions;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("")
			}

			fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
				match value {
					1 => Ok(Permissions::Write),
					2 => Ok(Permissions::Read),
					3 => Ok(Permissions::Sudo),
					_ => Ok(Permissions::None)
				}
			}

			fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E>  {
				if value < 0 {
					return Ok(Permissions::None);
				}
				self.visit_u64(value as u64)
			}

			fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E>  {
				match value.to_lowercase().as_str() {
					"sudo" => Ok(Permissions::Sudo),
					"read" => Ok(Permissions::Read),
					"write" => Ok(Permissions::Write),
					_ => Ok(Permissions::None)
				}
			}
		}

		deserializer.deserialize_any(PermissionsVisitor)
	}
}
