use std::{io::{Read, Write}, sync::{LazyLock, atomic::{AtomicU64, Ordering}}};
use axum::{Extension, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use rsa::rand_core::{OsRng, RngCore};
use toml_edit::{DocumentMut, Item, Table};
use super::auth::Credentials;

type ApiError = (StatusCode, &'static str);
const MAX_CONFIG: usize = 1024 * 1024;
static SAVING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static REVISION_KEY: LazyLock<[u8; 32]> = LazyLock::new(|| {
	let mut key = [0; 32];
	OsRng.fill_bytes(&mut key);
	key
});

fn authorize(credentials: &Credentials) -> Result<bool, ApiError> {
	super::auth::require_sudo(credentials)?;
	let config = srvpro3_config::get().map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	if !config.http_api.webui { return Err((StatusCode::NOT_FOUND, "WebUI 未启用")); }
	Ok(config.http_api.webui)
}

fn read_file() -> Result<String, ApiError> {
	let file = std::fs::File::open("config.toml").map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取 config.toml 失败"))?;
	let mut text = String::new();
	file.take(MAX_CONFIG as u64 + 1).read_to_string(&mut text)
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "配置文件不是有效的 UTF-8 文本或读取失败"))?;
	if text.len() > MAX_CONFIG { return Err((StatusCode::PAYLOAD_TOO_LARGE, "配置文件超过 1 MiB")); }
	Ok(text)
}

fn revision(text: &str) -> String {
	// 不向前端暴露含账号配置的裸哈希。
	format!("{:x}", Sha256::new().chain_update(*REVISION_KEY).chain_update(text.as_bytes()).finalize())
}

fn document(text: &str) -> Result<DocumentMut, ApiError> {
	text.parse().map_err(|_| (StatusCode::BAD_REQUEST, "TOML 格式不正确"))
}

// JSON 对象的键序不保证普通字段在子表前；用文档结构生成合法的 TOML。
fn config_document(values: &serde_json::Value) -> Result<DocumentMut, ApiError> {
	fn table(values: &serde_json::Value) -> Result<Table, ApiError> {
		let values = values.as_object().ok_or((StatusCode::BAD_REQUEST, "配置表必须是对象"))?;
		let mut result = Table::new();
		for (key, value) in values {
			let item = if value.is_object() { Item::Table(table(value)?) } else { Item::Value(scalar(value)?) };
			result.insert(key, item);
		}
		Ok(result)
	}
	fn scalar(value: &serde_json::Value) -> Result<toml_edit::Value, ApiError> {
		Ok(match value {
			serde_json::Value::String(value) => value.as_str().into(),
			serde_json::Value::Bool(value) => (*value).into(),
			serde_json::Value::Number(value) => value.as_i64()
				.ok_or((StatusCode::BAD_REQUEST, "配置数字必须是 TOML 范围内的整数"))?.into(),
			serde_json::Value::Array(values) => {
				let mut array = toml_edit::Array::new();
				for value in values { array.push(scalar(value)?); }
				array.into()
			}
			_ => return Err((StatusCode::BAD_REQUEST, "配置字段类型不正确")),
		})
	}
	let mut document = DocumentMut::new();
	*document.as_table_mut() = table(values)?;
	Ok(document)
}

#[derive(Serialize)]
pub struct ConfigResponse { values: serde_json::Value, revision: String }

fn public_values(text: &str) -> Result<serde_json::Value, ApiError> {
	let config: srvpro3_config::Config = basic_toml::from_str(text)
		.map_err(|_| (StatusCode::BAD_REQUEST, "配置无法解析"))?;
	let mut values = serde_json::to_value(config)
		.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "配置转换失败"))?;
	if let Some(api) = values.get_mut("http_api").and_then(serde_json::Value::as_object_mut) { api.remove("user"); }
	Ok(values)
}

pub async fn read(Extension(credentials): Extension<Credentials>) -> Result<Json<ConfigResponse>, ApiError> {
	authorize(&credentials)?;
	tokio::task::spawn_blocking(|| {
		let content = read_file()?;
		Ok(Json(ConfigResponse { revision: revision(&content), values: public_values(&content)? }))
	}).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取配置任务失败"))?
}

#[derive(Deserialize)]
pub struct SaveRequest { values: serde_json::Value, revision: String }

pub async fn save(Extension(credentials): Extension<Credentials>, Json(request): Json<SaveRequest>) -> Result<Json<ConfigResponse>, ApiError> {
	authorize(&credentials)?;
	if request.values.to_string().len() > MAX_CONFIG { return Err((StatusCode::PAYLOAD_TOO_LARGE, "配置文件超过 1 MiB")); }
	// 锁由阻塞任务持有，客户端断开也不会导致两个保存任务并行。
	let guard = SAVING.lock().await;
	let result = tokio::task::spawn_blocking(move || {
		let _guard = guard;
		let webui = authorize(&credentials)?;
		let mut updated = config_document(&request.values)?;
		let content = updated.to_string();
		for key in ["http_api", "api"] {
			if updated.get(key).and_then(Item::as_table_like).is_some_and(|table| table.contains_key("user")) {
				return Err((StatusCode::FORBIDDEN, "不允许提交或修改 http_api.user"));
			}
		}
		let parsed: srvpro3_config::Config = basic_toml::from_str(&content)
			.map_err(|_| (StatusCode::BAD_REQUEST, "TOML 格式或配置字段类型不正确"))?;
		if parsed.http_api.webui != webui {
			return Err((StatusCode::FORBIDDEN, "不允许修改 http_api.webui"));
		}
		let current = read_file()?;
		if revision(&current) != request.revision {
			return Err((StatusCode::CONFLICT, "配置文件已发生变化，请重新读取后再保存"));
		}
		let original = document(&current)?;
		let original_config: srvpro3_config::Config = basic_toml::from_str(&current)
			.map_err(|_| (StatusCode::BAD_REQUEST, "服务器现有配置无法解析，请先在服务器修复"))?;
		if parsed.http_api.webui != original_config.http_api.webui {
			return Err((StatusCode::FORBIDDEN, "不允许修改 http_api.webui"));
		}
		if parsed.http_api.port != original_config.http_api.port {
			return Err((StatusCode::FORBIDDEN, "不允许修改 http_api.port"));
		}
		// 账号区完全由服务器恢复；覆盖别名、内联表和点分键等 TOML 写法。
		let original_key = if original.contains_key("http_api") { "http_api" } else { "api" };
		let target_key = if updated.contains_key("api") { "api" } else { "http_api" };
		if let Some(users) = original.get(original_key).and_then(Item::as_table_like).and_then(|table| table.get("user")) {
			if !updated.contains_key(target_key) { updated.insert(target_key, Item::Table(Table::new())); }
			updated.get_mut(target_key).and_then(Item::as_table_like_mut)
				.ok_or((StatusCode::BAD_REQUEST, "http_api 必须是配置表"))?.insert("user", users.clone());
		}
		let content = updated.to_string();
		if content.len() > MAX_CONFIG { return Err((StatusCode::PAYLOAD_TOO_LARGE, "配置文件超过 1 MiB")); }
		let _: srvpro3_config::Config = basic_toml::from_str(&content)
			.map_err(|_| (StatusCode::BAD_REQUEST, "合并后的配置无效"))?;
		let response = ConfigResponse { revision: revision(&content), values: public_values(&content)? };
		// 同目录临时文件写完后替换，避免重载读到半份配置。
		let path = format!(".config.toml.{}.{}.tmp", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed));
		let mut options = std::fs::OpenOptions::new();
		options.write(true).create_new(true);
		#[cfg(unix)] {
			use std::os::unix::fs::OpenOptionsExt;
			options.mode(0o600);
		}
		let mut file = options.open(&path).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "创建配置临时文件失败"))?;
		let written = file.write_all(content.as_bytes()).and_then(|_| file.sync_all());
		drop(file);
		let saved = written.and_then(|_| std::fs::rename(&path, "config.toml"));
		if saved.is_err() {
			let _ = std::fs::remove_file(&path);
			return Err((StatusCode::INTERNAL_SERVER_ERROR, "保存配置失败，原配置未替换"));
		}
		srvpro3_log::info!("管理员 {} 已保存 config.toml，等待重载生效", credentials.user);
		Ok(Json(response))
	}).await.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "保存配置任务失败"))?;
	result
}
