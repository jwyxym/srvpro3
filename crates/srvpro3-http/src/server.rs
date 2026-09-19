mod history;
mod room;
mod auth;
pub mod query;

use axum::{routing::get, middleware, Router, serve};
use tokio::{spawn, net::TcpListener};
use anyhow::Error;

use srvpro3_log::*;

pub async fn start(
	port: u16,
	room: bool,
	history: bool,
	database: bool
) -> Result<(), Error> {
	let app: Router = Router::new();
	let app: Router = if room {
		app.route("/room/list", get(room::list))
	} else {
		warn!("未启用HTTP<房间列表>接口，如需启用，请修改config.toml的内容");
		app
	};
	let app: Router = if history && database {
		app.route("/history/list", get(history::list))
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
