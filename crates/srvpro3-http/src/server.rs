mod auth;
mod cards;
mod history;
mod host;
mod query;
mod reload;
mod room;
mod ws;
mod webui;

use std::sync::LazyLock;
use anyhow::Error;
use axum::{Router, middleware, routing::{delete, get, post, put}, serve};
use parking_lot::RwLockReadGuard;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle, sync::MutexGuard};

use srvpro3_config::{Config, DB};
use srvpro3_log::*;

pub use reload::register_reload;

struct Server {
	port: u16,
	task: JoinHandle<()>
}

static SERVER: LazyLock<Mutex<Option<Server>>> = LazyLock::new(|| Mutex::new(None));

pub async fn reload() -> Result<(), Error> {
	let config: RwLockReadGuard<'_, Config> = srvpro3_config::get()?;
	let port: u16 = config.http_api.port;
	let room: bool = config.http_api.room;
	let history: bool = config.http_api.history;
	let ws: bool = config.http_api.ws;
	let database: bool = !matches!(config.db.db, DB::None);
	drop(config);

	let mut server: MutexGuard<'_, Option<Server>> = SERVER.lock().await;
	if server
		.as_ref()
		.is_some_and(|server: &Server| server.port == port && !server.task.is_finished()
	) {
		return Ok(());
	}
	if let Some(server) = server.take() {
		server.task.abort();
		let _ = server.task.await;
	}
	if port == 0 {
		warn!("未启用HTTP接口，如需启用，请修改config.toml的内容");
		return Ok(());
	}

	let app: Router = Router::new()
		.route("/ws", get(ws::connect))
		.route("/host", get(host::get))
		.route("/cards", get(cards::get))
		.route("/reload", post(reload::reload))
		.route("/room", get(room::list))
		.route("/room", delete(room::interrupt))
		.route("/history", get(history::list))
		.route("/history", post(history::create))
		.route("/history", put(history::update))
		.route("/history", delete(history::delete))
		.route_layer(middleware::from_fn(auth::authorize))
		.merge(webui::router());
	if !room || !ws {
		warn!("未启用WebSocket<房间列表>接口，如需启用，请修改config.toml的内容");
	}
	if !room {
		warn!("未启用HTTP<房间列表>接口，如需启用，请修改config.toml的内容");
	}
	if !history || !database {
		warn!("未启用HTTP<历史记录>接口，如需启用，请修改config.toml的内容");
	}
	let listener: TcpListener = TcpListener::bind(("0.0.0.0", port)).await?;
	let task: JoinHandle<()> = tokio::spawn(async move {
		if let Err(error) = serve(listener, app).await {
			error!("HTTP服务器运行失败，原因：{error}");
		}
	});
	*server = Some(Server { port, task });
	info!("HTTP服务器启动成功，监听在端口：{port}");
	Ok(())
}
