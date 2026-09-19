use axum::{extract::{Json, Query}, http::StatusCode};
use serde::{Deserialize, Serialize};
use srvpro3_database::history::Model;

use super::query::ListQuery;

#[derive(Serialize)]
pub struct ListResponse {
	list: Vec<Model>,
	total: u64,
}

#[derive(Deserialize)]
pub struct CreateRequest {
	pub player_a: String,
	pub player_b: String,
	pub deck_a: String,
	pub deck_b: String,
	pub winner_id: Option<String>,
	pub room_id: String,
	pub replay: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
	pub id: i64,
	pub winner_id: String,
	pub replay: String,
}

#[derive(Deserialize)]
pub struct DeleteQuery {
	pub id: Option<i64>,
	pub room_id: Option<String>,
	#[serde(default)]
	pub all: bool,
}

#[derive(Serialize)]
pub struct AffectedResponse {
	rows_affected: u64,
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

pub async fn create(
	Json(request): Json<CreateRequest>,
) -> Result<Json<Model>, (StatusCode, &'static str)> {
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let record: Model = srvpro3_database::history::create(
		&db, request.player_a, request.player_b, request.deck_a, request.deck_b,
		request.winner_id, request.room_id, request.replay,
	).await.map_err(|_| (StatusCode::BAD_REQUEST, "新增历史记录失败"))?;
	Ok(Json(record))
}

pub async fn update(
	Json(request): Json<UpdateRequest>,
) -> Result<Json<Model>, (StatusCode, &'static str)> {
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let record: Model = srvpro3_database::history::update(&db, request.id, request.winner_id, request.replay)
		.await.map_err(|_| (StatusCode::NOT_FOUND, "历史记录不存在或修改失败"))?;
	Ok(Json(record))
}

pub async fn delete(
	Query(query): Query<DeleteQuery>,
) -> Result<Json<AffectedResponse>, (StatusCode, &'static str)> {
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let result = match (query.id, query.room_id, query.all) {
		(Some(id), None, false) => srvpro3_database::history::delete::by_id(&db, id).await,
		(None, Some(room_id), false) => srvpro3_database::history::delete::by_room(&db, room_id).await,
		(None, None, true) => srvpro3_database::history::delete::all(&db).await,
		_ => return Err((StatusCode::BAD_REQUEST, "请只指定 id、room_id 或 all=true 其中之一")),
	}.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "删除历史记录失败"))?;
	Ok(Json(AffectedResponse { rows_affected: result.rows_affected }))
}
