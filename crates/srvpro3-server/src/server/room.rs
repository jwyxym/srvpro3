use std::{collections::{BTreeMap, BTreeSet, VecDeque}, sync::{Arc, Mutex}, time::{SystemTime, UNIX_EPOCH}};
use ygopro::host::DuelHost;
use ygopro_data::{data::Deck, constants::{CorePlayer, Netplayer, DuelStage}, message::{ctos, stoc, gm}};
use super::{history, player::PlayerRecord, DuelRecord};
use crate::rooms::{RoomList, RoomPlayer, ChatInfo};

pub struct RoomEntry {
	pub host: DuelHost,
	pub connections: usize,
	pub record: Arc<Mutex<RoomRecord>>,
}

#[derive(Debug)]
pub struct RoomRecord {
	pub room_id: String,
	/// 按连接 ID 保存，玩家同名或更换座位不会覆盖其他人的卡组。
	pub players: BTreeMap<u64, PlayerRecord>,
	/// 按局数保存，比赛换局时不会覆盖上一局的 winner。
	pub duels: BTreeMap<u32, DuelRecord>,
	// 内部 handler 可能先于 TCP 输出处理 TypeChange，用座位暂存已接受的卡组。
	accepted_decks: BTreeMap<u8, Deck>,
	pub current_history: Option<history::HistoryRecord>,
	pub history: Vec<history::HistoryRecord>,
	pub chats: VecDeque<ChatInfo>,
	pub team_size: u8,
	pub stage: DuelStage,
	pub side_timeout_secs: u64,
	pub engine: Option<tokio::sync::mpsc::WeakUnboundedSender<ygopro::duel::Request>>,
	pub refreshing: BTreeSet<u8>,
}

impl RoomRecord {
	pub fn new(room_id: String) -> Self {
		Self { room_id, players: BTreeMap::new(), duels: BTreeMap::new(), accepted_decks: BTreeMap::new(), current_history: None, history: Vec::new(), chats: VecDeque::new(), team_size: 1, stage: DuelStage::Begin, side_timeout_secs: 180, engine: None, refreshing: BTreeSet::new() }
	}

	// 调用时先锁记录，再锁列表；只更新仍存在的房间，避免结束后重新插入。
	pub fn update_info(&self, rooms: &RoomList, room_id: &str) {
		let updated = {
			let mut rooms = rooms.write();
			let Some(info) = rooms.get_mut(room_id) else { return };
			info.player_a.clear();
			info.player_b.clear();
			info.spectators = 0;
			info.connections = 0;
			for (&id, player) in &self.players {
				if !player.connected { continue; }
				info.connections += 1;
				match player.position {
					Netplayer::Player(slot) => {
						let team = if slot < self.team_size { &mut info.player_a } else { &mut info.player_b };
						team.push(RoomPlayer { id, name: player.name.clone(), slot });
					}
					Netplayer::Observer(_) => info.spectators += 1,
					_ => {}
				}
			}
			info.player_a.sort_by_key(|player| player.slot);
			info.player_b.sort_by_key(|player| player.slot);
			info.chats = self.chats.iter().cloned().collect();
			info.clone()
		};
		crate::rooms::updated(updated);
	}

	pub fn observe_input(&mut self, id: u64, message: &ctos::Message) {
		let Some(player) = self.players.get_mut(&id) else { return };
		if let ctos::Message::Chat(chat) = message {
			if player.connected && matches!(player.position, Netplayer::Player(_) | Netplayer::Observer(_)) {
				// 限制条数和单条长度，避免房间聊天无限占用内存。
				if self.chats.len() >= 100 { self.chats.pop_front(); }
				self.chats.push_back(ChatInfo {
					player_id: id,
					name: player.name.clone(),
					content: chat.msg.to_string().chars().take(1000).collect(),
					created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
				});
			}
		}
		if matches!(message, ctos::Message::HsNotReady(_)) {
			player.deck = None;
			if let Netplayer::Player(slot) = player.position { self.accepted_decks.remove(&slot); }
		}
	}

	pub fn store_accepted_deck(&mut self, slot: u8, deck: Option<Deck>) {
		if let Some(deck) = &deck {
			self.accepted_decks.insert(slot, deck.clone());
		} else {
			self.accepted_decks.remove(&slot);
		}
		for player in self.players.values_mut() {
			if player.connected && player.position == Netplayer::Player(slot) {
				player.deck = deck.clone();
				if self.stage == DuelStage::Begin || player.reconnect_deck.is_none() {
					player.reconnect_deck = deck.clone();
				}
			}
		}
	}

	pub fn observe_output(&mut self, id: u64, duel_number: &mut u32, message: &stoc::Message) {
		match message {
			stoc::Message::TypeChange(value) => {
				if let Some(player) = self.players.get_mut(&id) {
					player.position = value.player;
					player.is_host = value.host;
					player.deck = match value.player {
						Netplayer::Player(slot) => self.accepted_decks.get(&slot).cloned(),
						_ => None,
					};
				}
			}
			stoc::Message::GameMessage(value) => match &value.message {
				gm::Message::Start(start) => {
					*duel_number += 1;
					let side = match start.player_type {
						0 => CorePlayer::FirstAttackPlayer,
						1 => CorePlayer::SecondAttackPlayer,
						_ => return, // 观众不能作为 winner 对应的玩家。
					};
					let duel = self.duels.entry(*duel_number).or_default();
					let players = duel.duel_players.entry(side).or_default();
					if !players.contains(&id) { players.push(id); }
				}
				gm::Message::Win(win) => {
					// 中途加入的观众没有完整局数，不能用其消息覆盖玩家的结果。
					if !self.players.get(&id).is_some_and(|player| matches!(player.position, Netplayer::Player(_))) {
						return;
					}
					// 同一个 Win 会广播给多条连接，不能重复记录结算。
					let duel = self.duels.entry(*duel_number).or_default();
					if duel.winner.is_none() { duel.winner = Some(win.clone()); }
				}
				_ => {}
			},
			_ => {}
		}
	}
}
