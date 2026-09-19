use super::entity::Config;

use anyhow::{Error, anyhow};
use basic_toml::from_str;
use parking_lot::{RwLock, RwLockReadGuard};
use std::sync::OnceLock;

pub static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

pub fn init(text: &str) -> Result<(), Error> {
	let new_config: Config = from_str(text)?;

	if let Some(config) = CONFIG.get() {
		let mut current = config.write();
		*current = new_config;
	} else {
		CONFIG
			.set(RwLock::new(new_config))
			.map_err(|_| anyhow!("配置已初始化"))?;
	}

	Ok(())
}

pub fn get() -> Result<RwLockReadGuard<'static, Config>, Error> {
	let config: &RwLock<Config> = CONFIG.get().ok_or_else(|| anyhow!("配置尚未加载"))?;
	Ok(config.read())
}
