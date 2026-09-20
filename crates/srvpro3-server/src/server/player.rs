use tokio::sync::{mpsc, oneshot};
use ygopro_data::{data::Deck, constants::Netplayer};
use super::transport::Protocol;

#[derive(Debug)]
pub struct PlayerRecord {
	pub name: String,
	pub position: Netplayer,
	pub is_host: bool,
	pub connected: bool,
	pub protocol: Protocol,
	pub outgoing: mpsc::Sender<Vec<u8>>,
	pub close: Option<oneshot::Sender<()>>,
	pub deck: Option<Deck>,
	pub reconnect_deck: Option<Deck>,
}
