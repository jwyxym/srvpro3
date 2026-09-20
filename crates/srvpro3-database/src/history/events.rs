use std::sync::LazyLock;
use serde::Serialize;
use tokio::sync::broadcast;
use super::Model;

#[derive(Clone, Debug)]
pub enum Event {
	Add(Model),
	Update(Model),
	Delete(Deleted),
}

#[derive(Clone, Debug, Serialize)]
pub struct Deleted {
	pub id: Option<i64>,
	pub room_id: Option<String>,
	pub all: bool,
	pub rows_affected: u64,
}

static EVENTS: LazyLock<broadcast::Sender<Event>> = LazyLock::new(|| broadcast::channel(128).0);

pub fn subscribe() -> broadcast::Receiver<Event> {
	EVENTS.subscribe()
}

pub fn publish(event: Event) {
	let _ = EVENTS.send(event);
}
