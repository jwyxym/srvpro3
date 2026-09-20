use std::{collections::BTreeMap, sync::{Arc, LazyLock, Weak}};
use parking_lot::{RwLock, RawRwLock, lock_api::{RwLockReadGuard, RwLock as RwLockGetGuard}};
use serde::Serialize;
use anyhow::{Result, ensure};
use tokio::sync::{broadcast, mpsc, oneshot};

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
static CONTROL: LazyLock<RwLock<Option<mpsc::UnboundedSender<Interrupt>>>> =
	LazyLock::new(|| RwLock::new(None));
static EVENTS: LazyLock<broadcast::Sender<RoomEvent>> =
	LazyLock::new(|| broadcast::channel(128).0);

#[derive(Clone, Debug)]
pub enum RoomEvent {
	Add(RoomInfo),
	Update(RoomInfo),
	Close(RoomInfo),
}

pub struct Interrupt {
	pub room_id: String,
	pub result: oneshot::Sender<bool>,
}

pub fn register() -> RoomList {
	let rooms: Arc<RwLockGetGuard<RawRwLock, BTreeMap<String, RoomInfo>>> = Arc::new(RwLock::new(BTreeMap::new()));
	*CURRENT.write() = Arc::downgrade(&rooms);
	rooms
}

pub fn register_control(sender: mpsc::UnboundedSender<Interrupt>) {
	*CONTROL.write() = Some(sender);
}

pub async fn interrupt(room_id: String) -> Result<bool> {
	let sender = CONTROL.read().clone().ok_or_else(|| anyhow::anyhow!("对局服务器未启动"))?;
	let (result, receiver) = oneshot::channel();
	sender.send(Interrupt { room_id, result }).map_err(|_| anyhow::anyhow!("对局服务器未启动"))?;
	receiver.await.map_err(|_| anyhow::anyhow!("中断房间请求失败"))
}

pub fn subscribe() -> broadcast::Receiver<RoomEvent> {
	EVENTS.subscribe()
}

pub fn added(room: RoomInfo) {
	let _ = EVENTS.send(RoomEvent::Add(room));
}

pub fn updated(room: RoomInfo) {
	let _ = EVENTS.send(RoomEvent::Update(room));
}

pub fn closed(room: RoomInfo) {
	let _ = EVENTS.send(RoomEvent::Close(room));
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
