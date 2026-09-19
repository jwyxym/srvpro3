use ygopro_data::{data::Deck, constants::Netplayer};

#[derive(Debug)]
pub struct PlayerRecord {
	pub name: String,
	pub position: Netplayer,
	pub connected: bool,
	pub deck: Option<Deck>,
	pub reconnect_deck: Option<Deck>,
}
