use super::server;

use anyhow::Error;

use srvpro3_log::*;

pub async fn init() -> Result<(), Error> {
	if let Err(e) = server::start().await {
		error!("HTTP服务器启动失败 原因：{}", e);
	}
	Ok(())
}