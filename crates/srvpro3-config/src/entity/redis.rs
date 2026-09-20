use serde::{Deserialize, Serialize,};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Redis {
	pub address: String,
	pub port: String,
	pub password: String,
	pub db: String
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
