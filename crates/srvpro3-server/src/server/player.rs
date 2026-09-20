use tokio::sync::{mpsc, oneshot};
use ygopro_data::{data::Deck, constants::Netplayer};
use super::transport::Protocol;

#[derive(Debug)]
pub struct PlayerRecord {
	pub name: String,
	/// 仅通过服务器的一次性 WindBot 邀请验证后标记，不依赖玩家名字。
	pub is_bot: bool,
	pub position: Netplayer,
	pub is_host: bool,
	pub connected: bool,
	pub protocol: Protocol,
	pub outgoing: mpsc::Sender<Vec<u8>>,
	pub close: Option<oneshot::Sender<()>>,
	pub deck: Option<Deck>,
	pub reconnect_deck: Option<Deck>,
}
