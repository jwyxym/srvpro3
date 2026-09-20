use anyhow::Error;

use super::server;
use srvpro3_log::*;

pub async fn init() -> Result<(), Error> {
	if let Err(error) = reload().await {
		error!("HTTP服务器启动失败，原因：{error}");
	}
	Ok(())
}

pub async fn reload() -> Result<(), Error> {
	server::reload().await
}
