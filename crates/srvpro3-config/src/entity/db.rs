use serde::{
	de::{self, Visitor},
	Deserialize, Deserializer, Serialize,
};
use std::fmt;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Database {
	pub db: DB,
	pub address: String,
	pub user: String,
	pub port: String,
	pub password: String
}

impl Default for Database {
	fn default() -> Self {
		Self {
			db: DB::default(),
			address: "srvpro3.db".into(),
			user: String::new(),
			port: String::new(),
			password: String::new(),
		}
	}
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub enum DB {
	MySQL,
	#[default]
	SQLite3,
	PostgresSQL,
	None,
}

impl<'de> Deserialize<'de> for DB {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct DBVisitor;

		impl<'de> Visitor<'de> for DBVisitor {
			type Value = DB;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("")
			}

			fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E>  {
				match value.to_lowercase().as_str() {
					"postgresql" | "postgressql" | "postgres" => Ok(DB::PostgresSQL),
					"sqlite3" | "sqlite" => Ok(DB::SQLite3),
					"mysql" => Ok(DB::MySQL),
					_ => Ok(DB::None)
				}
			}
		}

		deserializer.deserialize_any(DBVisitor)
	}
}

impl Database {
	pub fn address(&self) -> Result<String, ()> {
		match self.db {
			DB::MySQL => Ok(format!(
				"mysql://{}:{}@{}:{}",
				self.user, self.password, self.address, self.port
			)),
			DB::PostgresSQL => Ok(format!(
				"postgres://{}:{}@{}:{}",
				self.user, self.password, self.address, self.port
			)),
			DB::SQLite3 => Ok({
				if self.address.starts_with("sqlite:") {
					self.address.clone()
				} else {
					format!("sqlite://{}?mode=rwc", self.address)
				}
			}),
			DB::None => Err(()),
		}
	}
}
