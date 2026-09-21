use serde::{Deserialize, Serialize,};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Redis {
	pub address: String,
	pub port: u16,
	pub password: String,
	pub db: String
}

impl Default for Redis {
	fn default() -> Self {
		Self {
			address: String::new(),
			port: 6379,
			password: String::new(),
			db: String::new(),
		}
	}
}

impl Redis {
	pub fn address(&self) -> Result<String, ()> {
		if self.address.trim().is_empty() {
			return Err(())
		}
		let password: &str = if self.password.trim().is_empty() {
			""
		} else {
			&format!(":{}@", self.password)
		};
		let db: &str = if self.db.trim().is_empty() {
			""
		} else {
			&format!("/{}", self.db)
		};
		Ok(format!(
			"redis://{}{}:{}{}",
			password, self.address, self.port, db
		))
	}
}
