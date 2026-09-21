pub mod crypto;

use axum::{
	extract::{Query, Request},
	http::{self, header, HeaderValue, Method, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
	body::Body
};
use serde::Deserialize;
use srvpro3_config::Permissions;

#[derive(Clone, Deserialize)]
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

fn check(credentials: &Credentials, method: &Method) -> Result<(), (StatusCode, &'static str)> {
	let config = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	let user = config.http_api.user.get(&credentials.user)
		.ok_or((StatusCode::UNAUTHORIZED, "账号或密码错误"))?;
	if user.password != credentials.password {
		return Err((StatusCode::UNAUTHORIZED, "账号或密码错误"));
	};
	let allowed: bool = if matches!(*method, Method::GET | Method::HEAD) {
		matches!(user.permissions, Permissions::Read | Permissions::Sudo)
	} else {
		matches!(user.permissions, Permissions::Write | Permissions::Sudo)
	};
	if !allowed { return Err((StatusCode::FORBIDDEN, "权限不足")); }
	Ok(())
}

#[derive(Deserialize)]
struct EncryptedQuery { auth: String }

async fn authenticate(request: &mut Request) -> Result<(), (StatusCode, &'static str)> {
	let Query(query) = Query::<EncryptedQuery>::try_from_uri(request.uri())
		.map_err(|_| (StatusCode::UNAUTHORIZED, "请使用公钥加密鉴权参数"))?;
	// auth 由客户端放在 query 最后，其余参数必须与密文中的请求目标一致。
	let parts: Vec<_> = request.uri().query().unwrap_or_default().split('&').filter(|part| !part.starts_with("auth=")).collect();
	let mut target = request.uri().path().to_owned();
	if !parts.is_empty() { target.push('?'); target.push_str(&parts.join("&")); }
	let credentials = crypto::decrypt(query.auth, request.method().to_string(), target).await?;
	check(&credentials, request.method())?;
	request.extensions_mut().insert(credentials);
	Ok(())
}

pub async fn authorize(mut request: Request, next: Next) -> Response {
	if request.method() == Method::OPTIONS { return headers(StatusCode::NO_CONTENT.into_response()); }
	let response = match authenticate(&mut request).await {
		Ok(()) => next.run(request).await,
		Err(error) => error.into_response(),
	};
	headers(response)
}

pub fn headers(mut response: http::Response<Body>) -> Response {
	response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
	response.headers_mut().insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
	response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
	response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"));
	response.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type"));
	response
}
