use axum::{extract::Query, http::StatusCode, Json};
use serde::Serialize;
use srvpro3_database::history::Model;

use super::query::ListQuery;

#[derive(Serialize)]
pub struct ListResponse {
	list: Vec<Model>,
	total: u64,
}

pub async fn list(
	Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, &'static str)> {
	if query.page_size == 0 || query.page.checked_mul(query.page_size).is_none() {
		return Err((StatusCode::BAD_REQUEST, "分页参数无效"));
	}

	let (list, total) = srvpro3_database::history::read::all(query.page, query.page_size)
		.await
		.map_err(|_| {
			(StatusCode::INTERNAL_SERVER_ERROR, "查询历史记录失败")
		})?;

	Ok(Json(ListResponse { list, total }))
}
