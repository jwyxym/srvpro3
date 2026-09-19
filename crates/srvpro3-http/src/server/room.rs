use axum::{extract::Query, http::StatusCode, Json};
use serde::Serialize;
use srvpro3_server::rooms::{self, RoomInfo};

use super::query::ListQuery;

#[derive(Serialize)]
pub struct ListResponse {
	list: Vec<RoomInfo>,
	total: u64,
}

pub async fn list(
	Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, &'static str)> {
	let (list, total) = rooms::get(query.page, query.page_size)
		.map_err(|_| (StatusCode::BAD_REQUEST, "分页参数无效"))?;
	Ok(Json(ListResponse { list, total }))
}
