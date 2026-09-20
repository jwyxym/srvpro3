use anyhow::{Error, anyhow};
use std::{fs, io::ErrorKind, sync::Arc, path::{Component, Path, PathBuf}};
use parking_lot::{lock_api::RwLockReadGuard, RawRwLock};

use super::{get, CardsSnapshot};

use srvpro3_config::Config;

fn valid_name(name: &str) -> bool {
	let path: &Path = Path::new(name);
	!path.is_absolute() && path.components().all(|component| !matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
}

pub fn script(name: &str) -> Result<Vec<u8>, Error> {
	if !valid_name(name) { return Err(anyhow!("脚本名称无效")); }

	let snapshot: Arc<CardsSnapshot> = get()?;
	for pack in &snapshot.packs {
		if let Some(content) = pack.script(name)? {
			return Ok(content);
		}
	}

	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()?;
	for expansion in &config.cards.expansions {
		let path: PathBuf = Path::new(expansion).join(name);
		match fs::read(path) {
			Ok(content) => return Ok(content),
			Err(error) if error.kind() == ErrorKind::NotFound => {}
			Err(error) => return Err(error.into()),
		}
	}

	Err(anyhow!("未找到脚本：{name}"))
}
