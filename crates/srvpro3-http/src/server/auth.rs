use axum::{extract::{Query, Request}, http::{header, HeaderValue, Method, StatusCode}, middleware::Next, response::{IntoResponse, Response}};
use serde::Deserialize;
use srvpro3_config::Permissions;

#[derive(Deserialize)]
struct Credentials {
	#[serde(alias = "username")]
	user: String,
	password: String,
}

fn check(request: &Request) -> Result<(), (StatusCode, &'static str)> {
	let Query(credentials) = Query::<Credentials>::try_from_uri(request.uri())
		.map_err(|_| (StatusCode::UNAUTHORIZED, "账号或密码错误"))?;
	let config = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	let user = config.http_api.user.get(&credentials.user)
		.ok_or((StatusCode::UNAUTHORIZED, "账号或密码错误"))?;
	if user.password != credentials.password {
		return Err((StatusCode::UNAUTHORIZED, "账号或密码错误"));
	};
	let allowed: bool = if matches!(*request.method(), Method::GET | Method::HEAD) {
		matches!(user.permissions, Permissions::Read | Permissions::Sudo)
	} else {
		matches!(user.permissions, Permissions::Write | Permissions::Sudo)
	};
	if !allowed { return Err((StatusCode::FORBIDDEN, "权限不足")); }
	Ok(())
}

pub async fn authorize(request: Request, next: Next) -> Response {
	let mut response = match check(&request) {
		Ok(()) => next.run(request).await,
		Err(error) => error.into_response(),
	};
	response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response.headers_mut().insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	response
}
