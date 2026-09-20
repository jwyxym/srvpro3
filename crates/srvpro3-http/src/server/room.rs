use axum::{extract::Query, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use parking_lot::{RawRwLock, lock_api::RwLockReadGuard};

use super::query::ListQuery;

use srvpro3_server::rooms::{self, RoomInfo};
use srvpro3_config::Config;

#[derive(Serialize)]
pub struct ListResponse {
	list: Vec<RoomInfo>,
	total: u64,
}

#[derive(Deserialize)]
pub struct InterruptQuery {
	pub room_id: String,
}

#[derive(Serialize)]
pub struct InterruptResponse {
	interrupted: bool,
}

fn check() -> Result<(), (StatusCode, &'static str)> {
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	if config.http_api.history { Ok(()) } else { Err((StatusCode::NOT_FOUND, "房间接口未启用")) }
}

pub async fn list(
	Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, &'static str)> {
	check()?;
	let (list, total) = rooms::get(query.page, query.page_size)
		.map_err(|_| (StatusCode::BAD_REQUEST, "分页参数无效"))?;
	Ok(Json(ListResponse { list, total }))
}

pub async fn interrupt(
	Query(query): Query<InterruptQuery>,
) -> Result<Json<InterruptResponse>, (StatusCode, &'static str)> {
	check()?;
	let interrupted: bool = rooms::interrupt(query.room_id).await
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "对局服务器未启动"))?;
	if !interrupted { return Err((StatusCode::NOT_FOUND, "房间不存在或无法中断")); }
	Ok(Json(InterruptResponse { interrupted }))
}
