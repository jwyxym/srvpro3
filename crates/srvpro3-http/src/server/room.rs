use axum::{extract::Query, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use srvpro3_server::rooms::{self, RoomInfo};

use super::query::ListQuery;

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

pub async fn list(
	Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, &'static str)> {
	let (list, total) = rooms::get(query.page, query.page_size)
		.map_err(|_| (StatusCode::BAD_REQUEST, "分页参数无效"))?;
	Ok(Json(ListResponse { list, total }))
}

pub async fn interrupt(
	Query(query): Query<InterruptQuery>,
) -> Result<Json<InterruptResponse>, (StatusCode, &'static str)> {
	let interrupted: bool = rooms::interrupt(query.room_id).await
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "对局服务器未启动"))?;
	if !interrupted { return Err((StatusCode::NOT_FOUND, "房间不存在或无法中断")); }
	Ok(Json(InterruptResponse { interrupted }))
}
