use serde::{Deserialize, Serialize};
use anyhow::{Result, ensure};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReplayStorageMode {
	#[default]
	Database,
	Local,
	S3,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ReplayStorage {
	pub mode: ReplayStorageMode,
	pub local: LocalReplayStorage,
	pub s3: S3ReplayStorage,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct LocalReplayStorage {
	pub directory: String,
}

impl Default for LocalReplayStorage {
	fn default() -> Self {
		Self { directory: "replay".into() }
	}
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct S3ReplayStorage {
	pub public_url: String,
	pub endpoint: String,
	pub region: String,
	pub bucket: String,
	pub prefix: String,
	pub access_key_id: String,
	pub secret_access_key: String,
	pub session_token: String,
	pub path_style: bool,
	pub presign_download: bool,
	pub presign_expiry: u32,
	pub timeout: u64,
}

impl Default for S3ReplayStorage {
	fn default() -> Self {
		Self {
			public_url: String::new(),
			endpoint: String::new(),
			region: "us-east-1".into(),
			bucket: String::new(),
			prefix: "replays".into(),
			access_key_id: String::new(),
			secret_access_key: String::new(),
			session_token: String::new(),
			path_style: true,
			presign_download: false,
			presign_expiry: 900,
			timeout: 30,
		}
	}
}

impl ReplayStorage {
	pub fn validate(&self) -> Result<()> {
		match self.mode {
			ReplayStorageMode::Database => {},
			ReplayStorageMode::Local => ensure!(!self.local.directory.trim().is_empty(), "本地录像目录不能为空"),
			ReplayStorageMode::S3 => self.s3.validate()?,
		}
		Ok(())
	}
}

impl S3ReplayStorage {
	pub fn validate(&self) -> Result<()> {
		ensure!(!self.bucket.trim().is_empty() && !self.region.trim().is_empty(), "S3 bucket 和 region 不能为空");
		ensure!(!self.access_key_id.is_empty() && !self.secret_access_key.is_empty(), "S3 访问密钥不能为空");
		ensure!((1..=604800).contains(&self.presign_expiry), "S3 签名 URL 有效期必须为 1 到 604800 秒");
		ensure!((1..=300).contains(&self.timeout), "S3 超时必须为 1 到 300 秒");
		Ok(())
	}
}
