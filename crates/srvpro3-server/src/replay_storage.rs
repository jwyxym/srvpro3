use std::{collections::HashMap, io::{Cursor, Read}, path::{Component, Path, PathBuf}, time::Duration};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use s3::{Bucket, Region, creds::Credentials, request::ResponseData};
use tokio::{fs::{self, File, OpenOptions}, io::{AsyncReadExt, AsyncWriteExt}};
use srvpro3_config::{ReplayStorage, ReplayStorageMode, S3ReplayStorage};

pub const DATABASE_PREFIX: &str = "yrp3d:v1:";
const LOCAL_PREFIX: &str = "local:v1:";
const S3_PREFIX: &str = "s3:v1:";
const MAX_REPLAY: usize = 64 * 1024 * 1024;

fn config() -> Result<ReplayStorage> {
	Ok(srvpro3_config::get()?.replay_storage.clone())
}

/// 启动、重载和 WebUI 保存时先校验配置，不进行网络请求。
pub fn validate_settings(settings: &ReplayStorage) -> Result<()> {
	settings.validate()?;
	if settings.mode == ReplayStorageMode::S3 {
		let prefix: &str = settings.s3.prefix.trim_matches('/');
		if !prefix.is_empty() { valid_key(prefix)?; }
		let _: Box<Bucket> = bucket(&settings.s3)?;
	}
	Ok(())
}

fn valid_key(key: &str) -> Result<()> {
	ensure!(!key.is_empty() && !key.contains('\\') && key.is_ascii(), "录像路径无效");
	ensure!(key.bytes().all(|value: u8| value.is_ascii_alphanumeric() || b"/-_.".contains(&value)), "录像路径包含无效字符");
	ensure!(Path::new(key).components().all(|part: Component<'_>| matches!(part, Component::Normal(_))), "录像路径必须是安全的相对路径");
	ensure!(key.split('/').all(|part: &str| !part.is_empty() && part != "." && part != ".."), "录像路径无效");
	Ok(())
}

pub fn local_directory() -> Result<PathBuf> {
	let directory: PathBuf = PathBuf::from(config()?.local.directory);
	ensure!(!directory.as_os_str().is_empty(), "本地录像目录不能为空");
	if directory.is_absolute() { return Ok(directory); }
	let executable: PathBuf = std::env::current_exe().context("获取服务端可执行文件路径失败")?;
	Ok(executable.parent().context("获取服务端所在目录失败")?.join(directory))
}

/// 校验规范化后的路径，禁止通过路径穿越或符号链接读取录像目录外的文件。
pub async fn local_file(key: &str) -> Result<PathBuf> {
	valid_key(key)?;
	ensure!(key.ends_with(".yrp3d.gz"), "只允许下载录像文件");
	let directory: PathBuf = fs::canonicalize(local_directory()?).await.context("录像目录不存在")?;
	let path: PathBuf = fs::canonicalize(directory.join(key)).await.context("录像文件不存在")?;
	ensure!(path.starts_with(&directory), "录像文件超出配置目录");
	ensure!(fs::metadata(&path).await?.is_file(), "录像路径不是文件");
	Ok(path)
}

fn bucket(settings: &S3ReplayStorage) -> Result<Box<Bucket>> {
	settings.validate()?;
	let token: Option<&str> = if settings.session_token.is_empty() { None } else { Some(&settings.session_token) };
	let credentials: Credentials = Credentials::new(Some(&settings.access_key_id), Some(&settings.secret_access_key), None, token, None)
		.context("S3 凭据配置无效")?;
	let region: Region = if settings.endpoint.is_empty() {
		settings.region.parse().context("S3 region 无效")?
	} else {
		let endpoint: reqwest::Url = settings.endpoint.parse().context("S3 endpoint 必须是完整 URL")?;
		ensure!(matches!(endpoint.scheme(), "http" | "https") && endpoint.host_str().is_some()
			&& endpoint.username().is_empty() && endpoint.password().is_none()
			&& endpoint.query().is_none() && endpoint.fragment().is_none(), "S3 endpoint 无效");
		Region::Custom { region: settings.region.clone(), endpoint: settings.endpoint.trim_end_matches('/').to_owned() }
	};
	let mut bucket: Box<Bucket> = Bucket::new(&settings.bucket, region, credentials).context("初始化 S3 bucket 失败")?;
	if settings.path_style { bucket = bucket.with_path_style(); }
	bucket.set_request_timeout(Some(Duration::from_secs(settings.timeout)));
	Ok(bucket)
}

pub fn database_bytes(encoded: &str) -> Result<Vec<u8>> {
	ensure!(encoded.len() <= MAX_REPLAY * 4 / 3 + 16, "录像文件过大");
	let encoded: &str = encoded.strip_prefix(DATABASE_PREFIX).context("不支持旧版观战帧，请下载新保存的 .yrp3d 录像")?;
	let bytes: Vec<u8> = STANDARD.decode(encoded).context("录像 Base64 格式无效")?;
	validate_gzip(&bytes)?;
	Ok(bytes)
}

fn validate_gzip(bytes: &[u8]) -> Result<()> {
	ensure!(bytes.len() <= MAX_REPLAY, "录像文件过大");
	ensure!(bytes.starts_with(&[0x1f, 0x8b, 0x08]), "录像 gzip 格式无效");
	Ok(())
}

/// 只接受旧接口上传的录像正文，禁止客户端伪造其他录像的存储引用。
pub async fn store_encoded(encoded: String) -> Result<String> {
	ensure!(!encoded.starts_with(LOCAL_PREFIX) && !encoded.starts_with(S3_PREFIX), "请上传录像正文，不能提交存储引用");
	let settings: ReplayStorage = config()?;
	if settings.mode == ReplayStorageMode::Database { return Ok(encoded); }
	let bytes: Vec<u8> = database_bytes(&encoded)?;
	store_with_config(&bytes, settings).await
}

pub async fn store(bytes: &[u8]) -> Result<String> {
	store_with_config(bytes, config()?).await
}

pub fn database_reference(bytes: &[u8]) -> Result<String> {
	validate_gzip(bytes)?;
	Ok(format!("{DATABASE_PREFIX}{}", STANDARD.encode(bytes)))
}

async fn store_with_config(bytes: &[u8], settings: ReplayStorage) -> Result<String> {
	validate_gzip(bytes)?;
	if settings.mode == ReplayStorageMode::Database {
		return database_reference(bytes);
	}
	let random: u128 = rand::random();
	let key: String = format!("{}/{random:032x}.yrp3d.gz", Utc::now().format("%Y/%m/%d"));
	match settings.mode {
		ReplayStorageMode::Database => unreachable!(),
		ReplayStorageMode::Local => {
			let directory: PathBuf = PathBuf::from(&settings.local.directory);
			let directory: PathBuf = if directory.is_absolute() { directory } else {
				std::env::current_exe()?.parent().context("获取服务端目录失败")?.join(directory)
			};
			let path: PathBuf = directory.join(&key);
			fs::create_dir_all(path.parent().context("录像目录无效")?).await?;
			let temporary: PathBuf = path.with_extension("tmp");
			let mut file: File = OpenOptions::new().write(true).create_new(true).open(&temporary).await?;
			let result: Result<()> = async {
				file.write_all(bytes).await?;
				file.sync_all().await?;
				drop(file);
				fs::rename(&temporary, &path).await?;
				Ok(())
			}.await;
			if result.is_err() { let _ = fs::remove_file(&temporary).await; }
			result.context("保存本地录像失败")?;
			Ok(format!("{LOCAL_PREFIX}{key}"))
		},
		ReplayStorageMode::S3 => {
			let prefix: &str = settings.s3.prefix.trim_matches('/');
			let key: String = if prefix.is_empty() { key } else { format!("{prefix}/{key}") };
			valid_key(&key)?;
			let bucket: Box<Bucket> = bucket(&settings.s3)?;
			let response: ResponseData = bucket.put_object_with_content_type(&key, bytes, "application/gzip").await.context("上传 S3 录像失败")?;
			ensure!((200..300).contains(&response.status_code()), "上传 S3 录像失败，HTTP {}", response.status_code());
			Ok(format!("{S3_PREFIX}{key}"))
		},
	}
}

/// 数据库存储没有外部 URL；S3 每次生成新签名，不缓存签名 URL。
pub async fn download_url(reference: &str) -> Result<Option<String>> {
	if let Some(key) = reference.strip_prefix(LOCAL_PREFIX) {
		valid_key(key)?;
		return Ok(Some(format!("/replay/{key}")));
	}
	if let Some(key) = reference.strip_prefix(S3_PREFIX) {
		valid_key(key)?;
		let settings: S3ReplayStorage = config()?.s3;
		let bucket: Box<Bucket> = bucket(&settings)?;
		let mut query: HashMap<String, String> = HashMap::new();
		query.insert("response-content-disposition".into(), "attachment; filename=\"replay.yrp3d.gz\"".into());
		return Ok(Some(bucket.presign_get(key, settings.presign_expiry, Some(query)).await.context("生成 S3 下载 URL 失败")?));
	}
	Ok(None)
}

pub async fn load(reference: &str) -> Result<Vec<u8>> {
	let bytes: Vec<u8> = if let Some(key) = reference.strip_prefix(LOCAL_PREFIX) {
		let file: File = File::open(local_file(key).await?).await?;
		let mut bytes: Vec<u8> = Vec::new();
		file.take(MAX_REPLAY as u64 + 1).read_to_end(&mut bytes).await?;
		bytes
	} else if reference.starts_with(S3_PREFIX) {
		let url: String = download_url(reference).await?.context("缺少 S3 下载 URL")?;
		let timeout: u64 = config()?.s3.timeout;
		let client: reqwest::Client = reqwest::Client::builder().timeout(Duration::from_secs(timeout)).build()?;
		let mut response: reqwest::Response = client.get(url).send().await.context("读取 S3 录像失败")?
			.error_for_status().context("读取 S3 录像失败")?;
		ensure!(response.content_length().is_none_or(|length: u64| length <= MAX_REPLAY as u64), "录像文件过大");
		let mut bytes: Vec<u8> = Vec::new();
		while let Some(chunk) = response.chunk().await.context("读取 S3 录像内容失败")? {
			ensure!(bytes.len() + chunk.len() <= MAX_REPLAY, "录像文件过大");
			bytes.extend_from_slice(&chunk);
		}
		bytes
	} else {
		return database_bytes(reference);
	};
	validate_gzip(&bytes)?;
	Ok(bytes)
}

pub fn unpack(bytes: Vec<u8>) -> Result<Vec<u8>> {
	let mut buffer: Vec<u8> = Vec::new();
	let decoder: flate2::read::GzDecoder<Cursor<Vec<u8>>> = flate2::read::GzDecoder::new(Cursor::new(bytes));
	Read::take(decoder, MAX_REPLAY as u64 + 1).read_to_end(&mut buffer).context("解压录像失败")?;
	ensure!(buffer.len() <= MAX_REPLAY, "录像解压后过大");
	Ok(buffer)
}
