use std::{collections::BTreeMap, sync::{Arc, LazyLock, Weak}};
use parking_lot::{RwLock, RawRwLock, lock_api::{RwLockReadGuard, RwLock as RwLockGetGuard}};
use serde::Serialize;
use anyhow::{Result, ensure};

#[derive(Clone, Debug, Serialize)]
pub struct RoomInfo {
	pub room_id: String,
	pub connections: usize,
	pub player_a: Vec<RoomPlayer>,
	pub player_b: Vec<RoomPlayer>,
	pub spectators: usize,
	pub chats: Vec<ChatInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomPlayer {
	pub id: u64,
	pub name: String,
	pub slot: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChatInfo {
	pub player_id: u64,
	pub name: String,
	pub content: String,
	pub created_at: u64,
}

pub type RoomList = Arc<RwLock<BTreeMap<String, RoomInfo>>>;
static CURRENT: LazyLock<RwLock<Weak<RwLock<BTreeMap<String, RoomInfo>>>>> =
	LazyLock::new(|| RwLock::new(Weak::new()));

pub fn register() -> RoomList {
	let rooms: Arc<RwLockGetGuard<RawRwLock, BTreeMap<String, RoomInfo>>> = Arc::new(RwLock::new(BTreeMap::new()));
	*CURRENT.write() = Arc::downgrade(&rooms);
	rooms
}

pub fn get(page: u64, pagesize: u64) -> Result<(Vec<RoomInfo>, u64)> {
	ensure!(pagesize > 0, "范围必须大于0");
	let offset: u64 = page
		.checked_mul(pagesize)
		.ok_or_else(|| anyhow::anyhow!("页数溢出"))?;
	let Some(rooms) = CURRENT.read().upgrade() else { return Ok((Vec::new(), 0)); };
	let rooms: RwLockReadGuard<'_, RawRwLock, BTreeMap<String, RoomInfo>> = rooms.read();
	let total: u64 = rooms.len() as u64;
	if offset >= total {
		return Ok((Vec::new(), total));
	}
	let limit: usize = pagesize.min(total - offset) as usize;
	let list: Vec<RoomInfo> = rooms.values().skip(offset as usize).take(limit).cloned().collect();
	Ok((list, total))
}
