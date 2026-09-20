mod history;
mod room;
mod ws;
mod auth;
mod query;

use axum::{routing::{delete, get, post, put}, middleware, Router, serve};
use tokio::{spawn, net::TcpListener};
use anyhow::Error;
use parking_lot::RwLockReadGuard;

use srvpro3_config::{Config, DB};
use srvpro3_log::*;

pub async fn start() -> Result<(), Error> {
	let config: RwLockReadGuard<'_, Config> = srvpro3_config::get()?;
	let port: u16 = config.http_api.port;
	if port == 0 {
		warn!("未启用HTTP接口，如需启用，请修改config.toml的内容");
		return Ok(());
	}
	let room: bool = config.http_api.room;
	let history: bool = config.http_api.history;
	let ws: bool = config.http_api.ws;
	let database: bool = if let DB::None = config.db.db { false } else { true };
	drop(config);
	let app: Router = Router::new()
		.route("/ws", get(ws::connect))
		.route("/room", get(room::list))
		.route("/room", delete(room::interrupt))
		.route("/history", get(history::list))
		.route("/history", post(history::create))
		.route("/history", put(history::update))
		.route("/history", delete(history::delete));
	if !room || !ws {
		warn!("未启用WebSocket<房间列表>接口，如需启用，请修改config.toml的内容");
	};
	if !room {
		warn!("未启用HTTP<房间列表>接口，如需启用，请修改config.toml的内容");
	};
	if !history || !database {
		warn!("未启用HTTP<历史记录>接口，如需启用，请修改config.toml的内容");
	};
	let app: Router = app.layer(middleware::from_fn(auth::authorize));
	let listener: TcpListener = TcpListener::bind(format!("0.0.0.0:{}", port))
		.await?;

	spawn(async move {
		if let Err(e) = serve(listener, app).await {
			error!("HTTP服务器启动失败 原因：{}", e);
		}
	});
	info!("HTTP服务器启动成功 监听在端口：{}", port);
	Ok(())
}
