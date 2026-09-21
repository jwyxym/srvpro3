use std::{collections::BTreeMap, hash::{BuildHasher, Hash, Hasher, RandomState}, sync::{Arc, LazyLock, Mutex, Weak}, time::Duration};
use anyhow::{Error, anyhow, ensure};
use ygopro_data::{constants::{Color, DuelStage, Netplayer}, message::stoc};
use super::{room::RoomRecord, transport::Connection, reconnect::bytes};

struct Invitation {
	room_id: String,
	record: Weak<Mutex<RoomRecord>>,
}

static INVITATIONS: LazyLock<Mutex<BTreeMap<String, Invitation>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

// 短标记适配旧客户端的密码长度，避免随机房间被再次匹配到其他房间。
pub fn take_room(connection: &Connection) -> Result<Option<(String, Arc<Mutex<RoomRecord>>)>, Error> {
	let mut invitations = INVITATIONS.lock().unwrap();
	if !connection.handshake.pass.starts_with("~ai") { return Ok(None); }
	ensure!(connection.peer_ip.is_loopback(), "机器人入房标记仅限本机使用");
	let invitation = invitations.remove(&connection.handshake.pass).ok_or_else(|| anyhow!("机器人入房标记已失效"))?;
	let record = invitation.record.upgrade().ok_or_else(|| anyhow!("房间已关闭"))?;
	check_room(&record.lock().unwrap())?;
	Ok(Some((invitation.room_id, record)))
}

fn check_room(record: &RoomRecord) -> Result<(), Error> {
	ensure!(record.tournament.is_none(), "比赛房间不允许添加机器人");
	ensure!(record.stage == DuelStage::Begin, "只能在等待开局时添加机器人");
	let players = record.players.values().filter(|player| player.connected && matches!(player.position, Netplayer::Player(_))).count();
	ensure!(players < usize::from(record.team_size) * 2, "房间没有空余对战席位");
	Ok(())
}

fn reply(record: &Arc<Mutex<RoomRecord>>, id: u64, message: String) {
	if let Some(player) = record.lock().unwrap().players.get(&id) {
		let _ = player.outgoing.try_send(bytes(stoc::Chat { player: Color::Lightblue.into(), msg: message.into() }.into()));
	}
}

struct Pending(String);
impl Drop for Pending {
	fn drop(&mut self) { INVITATIONS.lock().unwrap().remove(&self.0); }
}

pub fn command(text: &str, record: &Arc<Mutex<RoomRecord>>, room_id: &str, id: u64) -> bool {
	let text = text.trim();
	let Some(name) = text.strip_prefix("/ai") else { return false };
	if !name.is_empty() && !name.starts_with(char::is_whitespace) { return false; }
	let name = name.trim().to_owned();
	let prepare = (|| -> Result<(u16, Pending), Error> {
		check_room(&record.lock().unwrap())?;
		let config = srvpro3_config::get()?;
		ensure!(config.server.tcp.port != 0, "添加 WindBot 需要启用 TCP 服务");
		ensure!(config.windbot.port != 0, "WindBot 未启用");
		let port = config.server.tcp.port;
		drop(config);
		let mut invitations = INVITATIONS.lock().unwrap();
		ensure!(!invitations.values().any(|value| value.record.ptr_eq(&Arc::downgrade(record))), "正在添加机器人，请稍后再试");
		let token = loop {
			let mut hasher = RandomState::new().build_hasher();
			room_id.hash(&mut hasher);
			let token = format!("~ai{:016x}", hasher.finish());
			if !invitations.contains_key(&token) { break token; }
		};
		invitations.insert(token.clone(), Invitation { room_id: room_id.to_owned(), record: Arc::downgrade(record) });
		Ok((port, Pending(token)))
	})();
	let (port, pending) = match prepare {
		Ok(value) => value,
		Err(error) => { reply(record, id, error.to_string()); return true; }
	};
	let record = record.clone();
	tokio::spawn(async move {
		let result = async {
			let selected = tokio::task::spawn_blocking(move || srvpro3_windbot::select(&name)).await??;
			check_room(&record.lock().unwrap())?;
			let bot_name = selected.name.clone();
			srvpro3_windbot::add(srvpro3_windbot::Bot {
				name: selected.name, deck: selected.ai_name, host: "127.0.0.1".into(), port,
				password: pending.0.clone(), dialog: Some(selected.dialog), version: None, hand: None, debug: false, chat: true,
			}).await?;
			Ok::<_, Error>(bot_name)
		}.await;
		match result {
			Ok(name) => {
				reply(&record, id, format!("已请求机器人「{name}」加入房间。"));
				// HTTP 返回后机器人可能尚未完成握手，暂时保留一次性入房标记。
				tokio::time::sleep(Duration::from_secs(30)).await;
			}
			Err(error) => reply(&record, id, format!("添加机器人失败：{error}")),
		}
		drop(pending);
	});
	true
}
