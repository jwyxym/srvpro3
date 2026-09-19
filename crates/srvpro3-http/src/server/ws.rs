use axum::{extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query}, response::Response};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use srvpro3_server::rooms::{self, RoomEvent};

use super::auth::{self, Credentials};

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

pub async fn connect(
	ws: WebSocketUpgrade,
	Query(credentials): Query<Credentials>,
) -> Response {
	ws.on_upgrade(move |socket| client(socket, credentials))
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
				Ok(RoomEvent::Close(room)) => if !send(&mut output, "close", room).await { break; },
				Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
					let (list, _) = rooms::get(0, u64::MAX).unwrap_or_default();
					if !send(&mut output, "all", list).await { break; }
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
			}
		}
	}
}
