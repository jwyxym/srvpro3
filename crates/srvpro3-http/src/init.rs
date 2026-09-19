use super::server;

use parking_lot::RwLockReadGuard;
use anyhow::Error;

use srvpro3_log::*;
use srvpro3_config::{Config, DB};

pub async fn init() -> Result<(), Error> {
	let config: RwLockReadGuard<'_, Config>  = srvpro3_config::get()?;
	let http_port: u16 = config.http_api.port;
	let room: bool = config.http_api.room;
	let history: bool = config.http_api.history;
	let ws: bool = config.http_api.ws;
	let database: bool = if let DB::None = config.db.db { false } else { true };
	drop(config);
	if http_port == 0 {
		warn!("未启用HTTP接口，如需启用，请修改config.toml的内容")
	} else {
		if let Err(e) = server::start(http_port, room, history, ws, database).await {
			error!("HTTP服务器启动失败 原因：{}", e);
		} else {
			info!("HTTP服务器启动成功 监听在端口：{}", http_port);
		}
	}

	Ok(())
}