use axum::{
	extract::{Query, Request},
	http::{self, header, HeaderValue, Method, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
	body::Body
};
use serde::Deserialize;
use srvpro3_config::Permissions;

#[derive(Deserialize)]
pub struct Credentials {
	#[serde(alias = "username")]
	pub user: String,
	pub password: String,
}

pub fn can_write(credentials: &Credentials) -> bool {
	let Ok(config) = srvpro3_config::get() else { return false };
	config.http_api.user.get(&credentials.user).is_some_and(|user| {
		user.password == credentials.password && matches!(user.permissions, Permissions::Write | Permissions::Sudo)
	})
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
	let mut response: http::Response<Body> = match check(&request) {
		Ok(()) => next.run(request).await,
		Err(error) => error.into_response(),
	};
	response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response.headers_mut().insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	response
}
