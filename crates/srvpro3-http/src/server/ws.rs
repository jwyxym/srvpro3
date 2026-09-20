use axum::{
	http::StatusCode,
	extract::{
		ws::{Message, WebSocket, WebSocketUpgrade}, Query
	},
	response::Response
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use parking_lot::{RawRwLock, lock_api::RwLockReadGuard};
use tokio::time::{Duration, MissedTickBehavior};

use super::{auth::{self, Credentials}, host};

use srvpro3_server::rooms::{self, RoomEvent};
use srvpro3_config::Config;

#[derive(Serialize)]
struct Outgoing<T> {
	#[serde(rename = "type")]
	kind: &'static str,
	msg: T,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "msg")]
enum Incoming {
	#[serde(rename = "interrupt")]
	Interrupt(String),
}

fn check() -> Result<(), (StatusCode, &'static str)> {
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	if config.http_api.ws && config.http_api.room { Ok(()) }
		else { Err((StatusCode::NOT_FOUND, "WebSocket接口未启用")) }
}

pub async fn connect(
	ws: WebSocketUpgrade,
	Query(credentials): Query<Credentials>,
) -> Result<Response, (StatusCode, &'static str)> {
	check()?;
	Ok(ws.on_upgrade(move |socket| client(socket, credentials)))
}

async fn send<T: Serialize>(socket: &mut futures::stream::SplitSink<WebSocket, Message>, kind: &'static str, msg: T) -> bool {
	let Ok(text) = serde_json::to_string(&Outgoing { kind, msg }) else { return false };
	socket.send(Message::Text(text.into())).await.is_ok()
}

async fn client(socket: WebSocket, credentials: Credentials) {
	let (mut output, mut input) = socket.split();
	let (list, _) = rooms::get(0, u64::MAX).unwrap_or_default();
	if !send(&mut output, "all", list).await { return; }
	let mut events = rooms::subscribe();
	let mut host_tick = tokio::time::interval(Duration::from_secs(1));
	host_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
	loop {
		tokio::select! {
			message = input.next() => {
				let Some(Ok(message)) = message else { break };
				let Message::Text(text) = message else { continue };
				let Ok(Incoming::Interrupt(room_id)) = serde_json::from_str::<Incoming>(&text) else {
					if !send(&mut output, "error", "消息格式无效").await { break; }
					continue;
				};
				if !auth::can_write(&credentials) {
					if !send(&mut output, "error", "权限不足").await { break; }
					continue;
				}
				match rooms::interrupt(room_id).await {
					Ok(true) => if !send(&mut output, "interrupt", true).await { break; },
					Ok(false) => if !send(&mut output, "error", "房间不存在或无法中断").await { break; },
					Err(_) => if !send(&mut output, "error", "对局服务器未启动").await { break; },
				}
			}
			event = events.recv() => match event {
				Ok(RoomEvent::Add(room)) => if !send(&mut output, "add", room).await { break; },
				Ok(RoomEvent::Update(room)) => if !send(&mut output, "update", room).await { break; },
				Ok(RoomEvent::Close(room)) => if !send(&mut output, "close", room).await { break; },
				Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
					let (list, _) = rooms::get(0, u64::MAX).unwrap_or_default();
					if !send(&mut output, "all", list).await { break; }
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
			},
			_ = host_tick.tick() => {
				let enabled = srvpro3_config::get().is_ok_and(|config| config.http_api.host);
				if enabled && !send(&mut output, "host", host::usage()).await { break; }
			}
		}
	}
}
