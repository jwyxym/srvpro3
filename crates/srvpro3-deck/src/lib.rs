use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Deck {
	pub main: Vec<u32>,
	pub extra: Vec<u32>,
	pub side: Vec<u32>,
}

impl Deck {
	pub fn validate(&self) -> Result<()> {
		anyhow::ensure!(!self.main.is_empty(), "主卡组不能为空");
		anyhow::ensure!(self.main.len() + self.extra.len() + self.side.len() <= 200, "卡组最多允许 200 张卡");
		anyhow::ensure!(!self.main.iter().chain(&self.extra).chain(&self.side).any(|&id| id == 0 || id > 0x0fff_ffff), "卡组包含无效卡号");
		Ok(())
	}

	pub fn count(&self, id: u32) -> u32 {
		self.main.iter().chain(&self.extra).chain(&self.side).filter(|&&code| code == id).count() as u32
	}
}

/// 读取存储格式中的卡片数量，按完整卡号匹配，包含主卡组、额外和副卡组。
pub fn card_count(encoded: &str, id: u32) -> Result<u32> {
	Ok(decode(encoded)?.count(id))
}

pub fn decode(encoded: &str) -> Result<Deck> {
	let bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(encoded).context("卡组编码无效")?;
	anyhow::ensure!(bytes.len() % 4 == 0, "卡组编码长度无效");
	anyhow::ensure!(bytes.len() <= 800, "卡组编码过长");
	let mut deck: Deck = Deck::default();
	for block in bytes.chunks_exact(4) {
		let value: u32 = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
		anyhow::ensure!((value >> 28) & 3 < 3, "卡组分区无效");
		let section: &mut Vec<u32> = match (value >> 28) & 3 { 0 => &mut deck.main, 1 => &mut deck.extra, _ => &mut deck.side };
		for _ in 0..((value >> 30) + 1) { section.push(value & 0x0fff_ffff); }
	}
	deck.validate()?;
	Ok(deck)
}

pub fn encode(ydk: &str) -> Result<String> {
	let mut sections: [Vec<u32>; 3] = Default::default();
	let mut section = 0;
	for (index, line) in ydk.lines().enumerate() {
		let line = line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
		match line {
			"#main" => section = 0,
			"#extra" => section = 1,
			"!side" => section = 2,
			_ if !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()) => {
				let id: u32 = line.parse::<u32>().with_context(|| format!("YDK 第 {} 行卡号超出 u32 范围", index + 1))?;
				sections[section].push(id);
			}
			_ => {}
		}
	}

	let mut bytes: Vec<u8> = Vec::new();
	for (section, cards) in sections.iter().enumerate() {
		let mut blocks: Vec<(u32, u32)> = Vec::new();
		let mut positions = HashMap::<u32, usize>::new();
		for &id in cards {
			if let Some(index) = positions.get(&id).copied() {
				blocks[index].1 += 1;
			} else {
				positions.insert(id, blocks.len());
				blocks.push((id, 1));
			}
		}
		for (id, count) in blocks {
			let value = (id & 0x0fff_ffff) | ((section as u32) << 28) | ((count - 1) << 30);
			bytes.extend_from_slice(&value.to_le_bytes());
		}
	}
	Ok(URL_SAFE_NO_PAD.encode(bytes))
}
