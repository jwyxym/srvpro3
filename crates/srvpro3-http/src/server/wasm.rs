use std::time::{Duration, Instant};
use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use srvpro3_deck::Deck as DeckData;
use srvpro3_plugin::{Deck, deck::Classification};
use tokio::{sync::{Semaphore, SemaphorePermit}, task::JoinHandle};

static TESTS: Semaphore = Semaphore::const_new(2);
type ApiError = (StatusCode, String);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestRequest {
	deck: DeckData,
}

#[derive(Serialize)]
pub struct TestResponse {
	classification: Classification,
	format: &'static str,
	elapsed_ms: u64,
	input: Value,
}

pub async fn test(Json(request): Json<TestRequest>) -> Result<Json<TestResponse>, ApiError> {
	request.deck.validate().map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
	let path: String = srvpro3_config::get().map_err(|error| (StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?.cards.wasm.trim().to_owned();
	if path.is_empty() { return Err((StatusCode::BAD_REQUEST, "服务器未配置分类 WASM，请先在设置中填写分类 WASM 路径".into())); }
	let permit: SemaphorePermit<'static> = TESTS.try_acquire().map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "WASM 测试繁忙，请稍后重试".into()))?;
	let task: JoinHandle<anyhow::Result<TestResponse>> = tokio::task::spawn_blocking(move || {
		// 超时后也持有许可到实际运行结束，避免不断堆积后台任务。
		let _permit: SemaphorePermit<'static> = permit;
		let started: Instant = Instant::now();
		let mut plugin: Deck = Deck::load(&path)?;
		let input: Value = plugin.input(&request.deck)?;
		let classification: Classification = plugin.classify(&request.deck)?;
		Ok(TestResponse { classification, input, format: plugin.format(), elapsed_ms: started.elapsed().as_millis() as u64 })
	});
	let result: anyhow::Result<TestResponse> = tokio::time::timeout(Duration::from_secs(30), task).await
		.map_err(|_| (StatusCode::GATEWAY_TIMEOUT, "WASM 测试超过 30 秒".into()))?
		.map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, format!("WASM 测试任务失败：{error}")))?;
	result.map(Json).map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, format!("{error:#}")))
}
