mod history;
mod room;
mod ws;
mod auth;
mod query;

use axum::{routing::{delete, get, post, put}, middleware, Router, serve};
use tokio::{spawn, net::TcpListener};
use anyhow::Error;

use srvpro3_log::*;

pub async fn start(
	port: u16,
	room: bool,
	history: bool,
	ws: bool,
	database: bool
) -> Result<(), Error> {
	let app: Router = Router::new();
	let app: Router = if room {
		app
			.route("/room", get(room::list))
			.route("/room", delete(room::interrupt))
	} else {
		warn!("未启用HTTP<房间列表>接口，如需启用，请修改config.toml的内容");
		app
	};
	let app: Router = if room && ws {
		app.route("/ws", get(ws::connect))
	} else {
		warn!("未启用WebSocket<房间列表>接口，如需启用，请修改config.toml的内容");
		app
	};
	let app: Router = if history && database {
		app
			.route("/history", get(history::list))
			.route("/history", post(history::create))
			.route("/history", put(history::update))
			.route("/history", delete(history::delete))
	} else {
		warn!("未启用HTTP<历史记录>接口，如需启用，请修改config.toml的内容");
		app
	};
	let app: Router = app.layer(middleware::from_fn(auth::authorize));
	let listener: TcpListener = TcpListener::bind(format!("0.0.0.0:{}", port))
		.await?;

	spawn(async move {
		if let Err(e) = serve(listener, app).await {
			error!("HTTP服务器启动失败 原因：{}", e);
		}
	});
	Ok(())
}
