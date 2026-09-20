use axum::{Router, extract::Request, body::Body, http::{self, StatusCode, header::{CACHE_CONTROL, HeaderValue, LOCATION}}, middleware::{self, Next}, response::{IntoResponse, Response}, routing::get};
use tower_http::services::{ServeDir, ServeFile};

async fn check(request: Request, next: Next) -> Response {
	let enabled: Result<bool, anyhow::Error> = srvpro3_config::get().map(|config| config.http_api.webui);
	let mut response: http::Response<Body> = match enabled {
		Ok(true) => next.run(request).await,
		Ok(false) => (StatusCode::NOT_FOUND, "WebUI未启用").into_response(),
		Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载").into_response(),
	};
	response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response
}

async fn redirect_home() -> impl IntoResponse {
	(StatusCode::MOVED_PERMANENTLY, [(LOCATION, "/")])
}

pub fn router() -> Router {
	Router::new()
		.route_service("/", ServeFile::new("webui/index.html"))
		.route("/login", get(redirect_home))
		.route("/home", get(redirect_home))
		.route("/home/room", get(redirect_home))
		.route("/hoom/history", get(redirect_home))
		.fallback_service(ServeDir::new("webui"))
		.layer(middleware::from_fn(check))
}
