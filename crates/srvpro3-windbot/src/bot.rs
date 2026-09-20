#[derive(Clone, Debug)]
pub struct Bot {
	pub name: String,
	pub deck: String,
	pub host: String,
	pub port: u16,
	pub password: String,
	pub dialog: Option<String>,
	pub version: Option<u16>,
	pub hand: Option<u8>,
	pub debug: bool,
	pub chat: bool
}