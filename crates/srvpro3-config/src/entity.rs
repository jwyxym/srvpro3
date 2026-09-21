mod db;
mod redis;
mod api;
mod server;
mod cards;
mod windbot;
mod tournament;

pub use server::Server;

pub use db::{Database, DB};
pub use redis::Redis;
pub use api::{HttpApi, User, Permissions};
pub use cards::Cards;
pub use windbot::WindBot;
pub use tournament::Tournament;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
	pub server: Server,
	pub db: Database,
	#[serde(alias = "api")]
	pub http_api: HttpApi,
	pub redis: Redis,
	pub cards: Cards,
	pub windbot: WindBot,
	pub tournament: Tournament,
}
