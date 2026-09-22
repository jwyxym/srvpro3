use axum::{extract::{Json, Query}, http::{header, StatusCode}, response::{IntoResponse, Response}};
use serde::{Deserialize, Serialize};
use parking_lot::{RawRwLock, lock_api::RwLockReadGuard};

use super::query::ListQuery;

use srvpro3_database::history::Model;
use srvpro3_config::Config;

static DOWNLOADS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[derive(Deserialize)]
pub struct DownloadQuery {
	pub id: i64,
}

pub async fn download(Query(query): Query<DownloadQuery>) -> Result<Response, (StatusCode, &'static str)> {
	check()?;
	if query.id <= 0 { return Err((StatusCode::BAD_REQUEST, "历史记录 ID 必须为正整数")); }
	let permit = DOWNLOADS.try_acquire().map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "录像下载繁忙，请稍后重试"))?;
	let _db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let record = tokio::time::timeout(std::time::Duration::from_secs(10), srvpro3_database::history::read::by_id(query.id))
		.await.map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "查询录像超时"))?
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询录像失败"))?
		.ok_or((StatusCode::NOT_FOUND, "历史记录不存在"))?;
	if record.replay.as_ref().is_none_or(|value| value.is_empty()) {
		return Err((StatusCode::NOT_FOUND, "该历史记录没有录像"));
	}
	let bytes = tokio::task::spawn_blocking(move || {
		let _permit = permit;
		srvpro3_server::export_replay(record)
	}).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "录像导出任务失败"))?
		.map_err(|error| {
			log::warn!("导出历史录像 {} 失败：{error:#}", query.id);
			(StatusCode::UNPROCESSABLE_ENTITY, "录像数据损坏或格式不支持，无法导出")
		})?;
	Ok(([
		// gzip 是文件内容，不设置 Content-Encoding，避免浏览器自动解压。
		(header::CONTENT_TYPE, "application/gzip".to_owned()),
		(header::CONTENT_DISPOSITION, format!("attachment; filename=\"replay-{}.yrp3d.gz\"", query.id)),
	], bytes).into_response())
}

#[derive(Serialize)]
pub struct ListResponse {
	list: Vec<RecordResponse>,
	total: u64,
}

/// 对外只提供录像是否存在，不发送录像正文；数据库仍保存完整内容。
#[derive(Serialize)]
pub struct RecordResponse {
	id: i64,
	player_a: String,
	player_b: String,
	deck_a: String,
	deck_b: String,
	player_c: Option<String>,
	player_d: Option<String>,
	deck_c: Option<String>,
	deck_d: Option<String>,
	winner_id: Option<String>,
	room_id: String,
	replay: bool,
	created_at: i64,
}

impl From<Model> for RecordResponse {
	fn from(record: Model) -> Self {
		Self {
			id: record.id,
			player_a: record.player_a,
			player_b: record.player_b,
			deck_a: record.deck_a,
			deck_b: record.deck_b,
			player_c: record.player_c,
			player_d: record.player_d,
			deck_c: record.deck_c,
			deck_d: record.deck_d,
			winner_id: record.winner_id,
			room_id: record.room_id,
			replay: record.replay.is_some_and(|value| !value.is_empty()),
			created_at: record.created_at,
		}
	}
}

#[derive(Deserialize)]
pub struct CreateRequest {
	pub player_a: String,
	pub player_b: String,
	pub deck_a: String,
	pub deck_b: String,
	pub player_c: Option<String>,
	pub player_d: Option<String>,
	pub deck_c: Option<String>,
	pub deck_d: Option<String>,
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

fn check() -> Result<(), (StatusCode, &'static str)> {
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	if config.http_api.history { Ok(()) } else { Err((StatusCode::NOT_FOUND, "历史记录接口未启用")) }
}

pub async fn list(
	Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, (StatusCode, &'static str)> {
	check()?;
	if query.page_size == 0 || query.page.checked_mul(query.page_size).is_none() {
		return Err((StatusCode::BAD_REQUEST, "分页参数无效"));
	}
	let (list, total) = srvpro3_database::history::read::all(query.page, query.page_size)
		.await
		.map_err(|_| {
			(StatusCode::INTERNAL_SERVER_ERROR, "查询历史记录失败")
		})?;
	Ok(Json(ListResponse { list: list.into_iter().map(RecordResponse::from).collect(), total }))
}

pub async fn create(
	Json(request): Json<CreateRequest>,
) -> Result<Json<RecordResponse>, (StatusCode, &'static str)> {
	check()?;
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let record: Model = srvpro3_database::history::create(
		&db, request.player_a, request.player_b, request.deck_a, request.deck_b,
		request.winner_id, request.room_id, request.replay,
		request.player_c, request.player_d, request.deck_c, request.deck_d,
	).await.map_err(|_| (StatusCode::BAD_REQUEST, "新增历史记录失败"))?;
	Ok(Json(record.into()))
}

pub async fn update(
	Json(request): Json<UpdateRequest>,
) -> Result<Json<RecordResponse>, (StatusCode, &'static str)> {
	check()?;
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let record: Model = srvpro3_database::history::update(&db, request.id, request.winner_id, request.replay)
		.await.map_err(|_| (StatusCode::NOT_FOUND, "历史记录不存在或修改失败"))?;
	Ok(Json(record.into()))
}

pub async fn delete(
	Query(query): Query<DeleteQuery>,
) -> Result<Json<AffectedResponse>, (StatusCode, &'static str)> {
	check()?;
	let db = srvpro3_database::db().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库未启用"))?;
	let result = match (query.id, query.room_id, query.all) {
		(Some(id), None, false) => srvpro3_database::history::delete::by_id(&db, id).await,
		(None, Some(room_id), false) => srvpro3_database::history::delete::by_room(&db, room_id).await,
		(None, None, true) => srvpro3_database::history::delete::all(&db).await,
		_ => return Err((StatusCode::BAD_REQUEST, "请只指定 id、room_id 或 all=true 其中之一")),
	}.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "删除历史记录失败"))?;
	Ok(Json(AffectedResponse { rows_affected: result.rows_affected }))
}
