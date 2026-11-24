use axum::{Json, extract::Query, http::StatusCode};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use parking_lot::RwLockReadGuard;
use srvpro3_config::{Config, DB};
use srvpro3_database::history::usage::{self, RoomType, Totals};
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};

type ApiError = (StatusCode, &'static str);
static QUERIES: Semaphore = Semaphore::const_new(4);

#[derive(Deserialize)]
pub struct UsageQuery {
	pub id: Option<u32>,
	#[serde(alias = "mode", alias = "type")]
	pub room_type: Option<RoomType>,
	#[serde(alias = "start_time")]
	pub start: Option<String>,
	#[serde(alias = "end_time")]
	pub end: Option<String>,
}

#[derive(Serialize)]
pub struct UsageResponse {
	id: Option<u32>,
	room_type: Option<RoomType>,
	start: i64,
	end: i64,
	timezone: &'static str,
	#[serde(flatten)]
	totals: Totals,
}

fn midnight(date: NaiveDate) -> Option<i64> {
	Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).earliest().map(|time: DateTime<Local>| time.timestamp())
}

fn timestamp(value: &str) -> Option<i64> {
	if let Ok(time) = value.parse::<i64>() { return (time >= 0).then_some(time); }
	if let Ok(time) = DateTime::parse_from_rfc3339(value) { return Some(time.timestamp()); }
	midnight(NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?)
}

pub async fn get(Query(query): Query<UsageQuery>) -> Result<Json<UsageResponse>, ApiError> {
	if query.id.is_none() { return Err((StatusCode::BAD_REQUEST, "缺少卡号 id")); }
	run(query).await
}

pub async fn distribution(Query(query): Query<UsageQuery>) -> Result<Json<UsageResponse>, ApiError> {
	run(query).await
}

async fn run(query: UsageQuery) -> Result<Json<UsageResponse>, ApiError> {
	if query.id.is_some_and(|id: u32| id == 0 || id > 0x0fff_ffff) { return Err((StatusCode::BAD_REQUEST, "卡号须为 1 到 268435455")); }
	{
		let config: RwLockReadGuard<'_, Config> = srvpro3_config::get().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
		if !config.http_api.history { return Err((StatusCode::NOT_FOUND, "历史记录接口未启用")); }
		if matches!(config.db.db, DB::None) { return Err((StatusCode::SERVICE_UNAVAILABLE, "数据库未启用")); }
	}
	let now: DateTime<Local> = Local::now();
	let first: NaiveDate = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
		.ok_or((StatusCode::INTERNAL_SERVER_ERROR, "无法确定当前月份"))?;
	let next: NaiveDate = if now.month() == 12 { NaiveDate::from_ymd_opt(now.year() + 1, 1, 1) }
		else { NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1) }
		.ok_or((StatusCode::INTERNAL_SERVER_ERROR, "无法确定下月日期"))?;
	let start: i64 = match query.start {
		Some(value) => timestamp(&value), None => midnight(first),
	}.ok_or((StatusCode::BAD_REQUEST, "开始时间无效，支持 YYYY-MM-DD、RFC3339 或 Unix 秒时间戳"))?;
	let end: i64 = match query.end {
		Some(value) => timestamp(&value), None => midnight(next),
	}.ok_or((StatusCode::BAD_REQUEST, "结束时间无效，支持 YYYY-MM-DD、RFC3339 或 Unix 秒时间戳"))?;
	if start >= end { return Err((StatusCode::BAD_REQUEST, "开始时间必须早于结束时间，统计范围为 [start, end)")); }
	let _permit: SemaphorePermit<'_> = QUERIES.try_acquire().map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "卡片使用率查询繁忙，请稍后重试"))?;
	let totals: Totals = tokio::time::timeout(Duration::from_secs(30), usage::calculate(query.id, query.room_type, start, end))
		.await.map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "卡片使用率查询超时，请缩小时间范围"))?
		.map_err(|error| {
			log::warn!("查询卡片使用率失败：{error:#}");
			(StatusCode::INTERNAL_SERVER_ERROR, "查询卡片使用率失败")
		})?;
	Ok(Json(UsageResponse { id: query.id, room_type: query.room_type, start, end, timezone: "server-local", totals }))
}
