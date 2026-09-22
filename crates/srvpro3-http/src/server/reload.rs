use std::sync::LazyLock;
use anyhow::Error;
use axum::{Extension, Json, http::StatusCode};
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

type ReloadResult = oneshot::Sender<Result<(), Error>>;

static CONTROL: LazyLock<RwLock<Option<mpsc::Sender<ReloadResult>>>> =
	LazyLock::new(|| RwLock::new(None));

// 交由主命令循环执行，避免与控制台重载、定时重载并行。
pub fn register_reload() -> mpsc::Receiver<ReloadResult> {
	let (sender, receiver) = mpsc::channel(1);
	*CONTROL.write() = Some(sender);
	receiver
}

#[derive(Serialize)]
pub struct ReloadResponse {
	reloaded: bool,
}

pub async fn reload(Extension(credentials): Extension<super::auth::Credentials>) -> Result<Json<ReloadResponse>, (StatusCode, &'static str)> {
	super::auth::require_sudo(&credentials)?;
	let sender = CONTROL.read().clone()
		.ok_or((StatusCode::SERVICE_UNAVAILABLE, "重载服务尚未启动"))?;
	let (result, receiver) = oneshot::channel();
	sender.try_send(result).map_err(|error| match error {
		mpsc::error::TrySendError::Full(_) => (StatusCode::TOO_MANY_REQUESTS, "已有重载请求等待处理"),
		mpsc::error::TrySendError::Closed(_) => (StatusCode::SERVICE_UNAVAILABLE, "重载服务已停止"),
	})?;
	receiver.await
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "重载请求未完成"))?
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "重载失败，请查看服务器日志"))?;
	Ok(Json(ReloadResponse { reloaded: true }))
}
