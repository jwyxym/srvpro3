use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListQuery {
	#[serde(default)]
	pub page: u64,
	#[serde(default = "default_page_size", alias = "pagesize")]
	pub page_size: u64,
}

fn default_page_size() -> u64 {
	60
}
