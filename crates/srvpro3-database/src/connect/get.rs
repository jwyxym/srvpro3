use super::{
	init::{DB, REDIS},
	entity::*
};

use parking_lot::RwLock;
use sea_orm::DatabaseConnection;
use anyhow::{Error, anyhow};
use redis::aio::ConnectionManager;

pub fn redis() -> Result<ConnectionManager, Error> {
	let redis: &RwLock<ConnectRedisBase> = REDIS.get().ok_or_else(|| anyhow!("Redis未启用"))?;
	let redis: ConnectionManager = redis.write().connection.clone();
	Ok(redis)
}

pub fn db() -> Result<DatabaseConnection, Error> {
	let db: &RwLock<ConnectDataBase> = DB.get().ok_or_else(|| anyhow!("数据库未启用"))?;
	let db: DatabaseConnection = db.read().db.clone();
	Ok(db)
}
