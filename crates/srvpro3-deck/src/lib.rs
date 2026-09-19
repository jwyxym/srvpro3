use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::collections::HashMap;

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