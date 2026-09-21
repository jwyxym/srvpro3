use std::{collections::HashMap, sync::LazyLock, time::{Duration, Instant}};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use parking_lot::Mutex;
use rsa::{Oaep, RsaPrivateKey, pkcs8::EncodePublicKey, rand_core::{OsRng, RngCore}};
use serde::Deserialize;
use sha2::Sha256;
use tokio::sync::{OnceCell, Semaphore};

use super::Credentials;

static KEY: OnceCell<RsaPrivateKey> = OnceCell::const_new();
static CHALLENGES: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static DECRYPTIONS: Semaphore = Semaphore::const_new(4);
const TTL: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
struct Envelope { key: String, iv: String, data: String }

#[derive(Deserialize)]
struct Payload {
	#[serde(flatten)]
	credentials: Credentials,
	challenge: String,
	method: String,
	target: String,
}

pub async fn init() -> anyhow::Result<()> {
	KEY.get_or_try_init(|| async {
		tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut OsRng, 2048)).await?
			.map_err(anyhow::Error::from)
	}).await?;
	Ok(())
}

pub async fn public_key() -> Response {
	let response = (|| {
		let key = KEY.get().ok_or((StatusCode::SERVICE_UNAVAILABLE, "鉴权尚未初始化"))?;
		let public_key = key.to_public_key().to_public_key_der()
			.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "无法获取鉴权公钥"))?;
		let mut challenges = CHALLENGES.lock();
		challenges.retain(|_, created| created.elapsed() < TTL);
		if challenges.len() >= 4096 { return Err((StatusCode::TOO_MANY_REQUESTS, "鉴权请求过于频繁，请稍后重试")); }
		let mut random = [0u8; 32];
		OsRng.fill_bytes(&mut random);
		let challenge = URL_SAFE_NO_PAD.encode(random);
		challenges.insert(challenge.clone(), Instant::now());
		Ok(Json(serde_json::json!({
			"public_key": URL_SAFE_NO_PAD.encode(public_key.as_bytes()),
			"challenge": challenge,
			"expires_in": TTL.as_secs(),
		})))
	})();
	super::headers(response.into_response())
}

pub async fn decrypt(auth: String, method: String, target: String) -> Result<Credentials, (StatusCode, &'static str)> {
	let invalid = (StatusCode::UNAUTHORIZED, "鉴权信息无效或已过期，请重新请求");
	if auth.len() > 16384 { return Err(invalid); }
	let permit = DECRYPTIONS.try_acquire().map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "鉴权请求过于频繁，请稍后重试"))?;
	let result = tokio::task::spawn_blocking(move || -> Option<Credentials> {
		let envelope: Envelope = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(auth).ok()?).ok()?;
		let key = KEY.get()?.decrypt_blinded(&mut OsRng, Oaep::new::<Sha256>(), &URL_SAFE_NO_PAD.decode(envelope.key).ok()?).ok()?;
		let iv: [u8; 12] = URL_SAFE_NO_PAD.decode(envelope.iv).ok()?.try_into().ok()?;
		let nonce = Nonce::from(iv);
		let data = URL_SAFE_NO_PAD.decode(envelope.data).ok()?;
		let plaintext = Aes256Gcm::new_from_slice(&key).ok()?.decrypt(&nonce, data.as_slice()).ok()?;
		let payload: Payload = serde_json::from_slice(&plaintext).ok()?;
		if payload.method != method || payload.target != target { return None; }
		// 原子消费挑战值，同一份密文最多只能验证一次（包括密码错误的请求）。
		let created = CHALLENGES.lock().remove(&payload.challenge)?;
		if created.elapsed() >= TTL { return None; }
		Some(payload.credentials)
	}).await;
	drop(permit);
	result.ok().flatten().ok_or(invalid)
}
