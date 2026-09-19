use sea_orm::DatabaseConnection;
use redis::aio::ConnectionManager;

#[derive(Debug)]
pub struct ConnectDataBase {
	pub db: DatabaseConnection,
	pub address: String,
}

pub struct ConnectRedisBase {
	pub connection: ConnectionManager,
	pub address: String,
}