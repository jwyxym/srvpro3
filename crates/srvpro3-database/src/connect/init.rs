use super::entity::*;
use crate::history;

use anyhow::{Error, anyhow};
use parking_lot::{RawRwLock, RwLock, RwLockReadGuard, lock_api::RwLockWriteGuard};
use sea_orm::{Database, DatabaseConnection};
use tokio::sync::OnceCell;
use redis::{Client, aio::ConnectionManager};

use srvpro3_log::*;
use srvpro3_config::Config;

pub static DB: OnceCell<RwLock<ConnectDataBase>> = OnceCell::const_new();
pub static REDIS: OnceCell<RwLock<ConnectRedisBase>> = OnceCell::const_new();

async fn redis(address: String) -> Result<(), Error> {
	async fn connect(address: String) -> Result<ConnectRedisBase, Error> {
		let client: Client = Client::open(address.clone())?;
		let connection: ConnectionManager = client.get_connection_manager().await?;
		Ok(ConnectRedisBase { connection, address })
	}
	if let Some(redis) = REDIS.get() {
		let mut redis: RwLockWriteGuard<'_, RawRwLock, ConnectRedisBase> = redis.write();
		if redis.address == address {
			return Ok(());
		}
		*redis = connect(address).await?;
	} else {
		REDIS.set(RwLock::new(connect(address).await?))
			.map_err(|e|anyhow!("{}", e))?;
	}
	info!("Redis连接成功");
	Ok(())
}

async fn db(address: String) -> Result<(), Error> {
	async fn connect(address: String) -> Result<ConnectDataBase, Error> {
		let db: DatabaseConnection = Database::connect(&address).await?;
		history::init(&db).await?;
		Ok(ConnectDataBase { db, address })
	}
	if let Some(db) = DB.get() {
		let mut db: RwLockWriteGuard<'_, RawRwLock, ConnectDataBase> = db.write();
		if db.address == address {
			return Ok(());
		}
		*db = connect(address).await?;
	} else {
		DB.set(RwLock::new(connect(address).await?))
			.map_err(|e|anyhow!("{}", e))?;
	}
	info!("数据库连接成功");
	Ok(())
}

pub async fn init() -> Result<(), Error> {
	let config: RwLockReadGuard<'_, Config>  = srvpro3_config::get()?;
	let database_address: Result<String, ()> = config.db.address();
	let redis_address: Result<String, ()> = config.redis.address();
	drop(config);
	match database_address {
		Ok(address) => {
			if let Err(e) = db(address).await {
				error!("数据库连接失败 原因：{}", e);
			}
		}
		Err(_) => {
			warn!("未启用数据库，如需启用，请修改config.toml的内容")
		}
	}
	match redis_address {
		Ok(address) => {
			if let Err(e) = redis(address).await {
				error!("Redis连接失败 原因：{}", e);
			}
		}
		Err(_) => {
			warn!("未启用Redis，如需启用，请修改config.toml的内容")
		}
	}
	Ok(())
}