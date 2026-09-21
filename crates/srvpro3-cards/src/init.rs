mod cdb;
mod lflist;
mod ypk;

use anyhow::Error;
use walkdir::WalkDir;
use parking_lot::RwLock;
use std::{collections::{BTreeMap, HashMap}, sync::Arc};
use tokio::sync::{OnceCell, broadcast};
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
static UPDATES: std::sync::LazyLock<broadcast::Sender<()>> = std::sync::LazyLock::new(|| broadcast::channel(16).0);

pub fn subscribe() -> broadcast::Receiver<()> { UPDATES.subscribe() }

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
	let _ = UPDATES.send(());
	Ok(())
}

pub async fn init() -> Result<(), Error> {
	let config: parking_lot::RwLockReadGuard<'_, Config> = srvpro3_config::get()?;
	let expansions: Vec<String> = config.cards.expansions.clone();
	let ypk: bool = config.cards.ypk;
	let excode: BTreeMap<u32, Vec<u16>> = config.cards.excode.clone();
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
							for card in cards {
								if snapshot.cards.contains_key(&card.code) {
									continue;
								}
								snapshot.cards.insert(card.code, card);
							}
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
	add_excode(&mut snapshot, excode);

	if ypk {
		info!("加载ypk数量: {}", snapshot.packs.len());
	} else {
		warn!("未启用ypk卡包，如需启用，请修改config.toml的内容");
	}
	info!("加载禁卡表数量: {}", snapshot.lflists.len());
	info!("加载卡片数量: {}", snapshot.cards.len());
	replace(snapshot)
}

fn add_excode(snapshot: &mut CardsSnapshot, excode: BTreeMap<u32, Vec<u16>>) {
	for (code, set_codes) in excode {
		if let Some(card) = snapshot.cards.get_mut(&code)
			&& let Some(mut i) = card.setcode.iter().position(|&x| x == 0) {
			for set_code in set_codes {
				if set_code == 0 {
					continue;
				}
				if i >= card.setcode.len() {
					break;
				}
				card.setcode[i] = set_code;
				i += 1;
			}
		};
	}
}
