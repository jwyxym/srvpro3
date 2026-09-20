mod cdb;
mod lflist;
mod ypk;

use anyhow::Error;
use walkdir::WalkDir;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::OnceCell;
use indexmap::IndexMap;
use ygopro_cdb_reader::Card;

use srvpro3_config::Config;
use srvpro3_log::*;

pub struct CardsSnapshot {
	pub cards: HashMap<u32, Card>,
	pub lflists: IndexMap<String, lflist::LFList>,
	pub packs: Vec<ypk::YPK>,
}

impl CardsSnapshot {
	fn new() -> Self {
		Self {
			cards: HashMap::new(),
			lflists: IndexMap::new(),
			packs: Vec::new(),
		}
	}
}

static SNAPSHOT: OnceCell<RwLock<Arc<CardsSnapshot>>> = OnceCell::const_new();

pub fn get() -> Result<Arc<CardsSnapshot>, Error> {
	let snapshot: &RwLock<Arc<CardsSnapshot>> = SNAPSHOT.get().ok_or_else(|| anyhow::anyhow!("卡片数据尚未加载"))?;
	Ok(snapshot.read().clone())
}

pub fn lflist_by_index(index: usize) -> Result<u32, Error> {
	get()?.lflists.get_index(index)
		.map(|(_, lflist)| lflist.hash)
		.ok_or_else(|| anyhow::anyhow!("禁限卡表编号不存在"))
}

pub fn lflist_by_name(name: &str) -> Result<u32, Error> {
	get()?.lflists.get(name)
		.map(|lflist| lflist.hash)
		.ok_or_else(|| anyhow::anyhow!("禁限卡表不存在：{name}"))
}

fn replace(snapshot: CardsSnapshot) -> Result<(), Error> {
	let snapshot: Arc<CardsSnapshot> = Arc::new(snapshot);
	if let Some(current) = SNAPSHOT.get() {
		*current.write() = snapshot;
	} else {
		SNAPSHOT.set(RwLock::new(snapshot)).map_err(|_| anyhow::anyhow!("卡片初始化失败"))?;
	}
	Ok(())
}

pub async fn init() -> Result<(), Error> {
	let config: parking_lot::RwLockReadGuard<'_, Config> = srvpro3_config::get()?;
	let expansions: Vec<String> = config.cards.expansions.clone();
	let ypk: bool = config.cards.ypk;
	drop(config);
	let mut snapshot: CardsSnapshot = CardsSnapshot::new();
	for i in expansions {
		for i in WalkDir::new(&i)
			.max_depth(1)
			.into_iter() {
			if let Ok(i) = i {
				if let Some(name) = i.file_name().to_str() {
					if name.ends_with(".cdb") {
						if let Ok(cards) = cdb::read(i.path()).await {
							for card in cards { snapshot.cards.insert(card.code, card); }
						}
					} else if name.ends_with("lflist.conf") {
						let _ = lflist::read_path(&mut snapshot.lflists, i.path());
					} else if ypk && (name.ends_with(".ypk") || name.ends_with(".zip")) {
						let _ = ypk::read(&mut snapshot, i.path());
					}
				}
			}
		}
	}
	if ypk {
		info!("加载ypk数量: {}", snapshot.packs.len());
	} else {
		warn!("未启用ypk卡包，如需启用，请修改config.toml的内容");
	}
	info!("加载禁卡表数量: {}", snapshot.lflists.len());
	info!("加载卡片数量: {}", snapshot.cards.len());
	replace(snapshot)
}
