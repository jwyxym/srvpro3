use anyhow::Error;
use lazy_static::lazy_static;
use regex::Regex;
use indexmap::IndexMap;
use std::{fs::read_to_string, path::Path, collections::HashMap};

lazy_static! {
	pub static ref COMMENTS_REGEX: Regex = Regex::new(r"#.*").unwrap();
	pub static ref HASH: u32 = {
		let mut h: u32 = 2166136261;
		for byte in "genesys".bytes() {
			h ^= byte as u32;
			h = h.wrapping_mul(16777619);
		}
		h
	};
}

#[derive(Clone, Debug)]
pub struct LFList {
	pub genesys: u32,
	pub hash: u32,
	pub glist: HashMap<u32, u32>,
	pub lflist: HashMap<u32, u8>,
}

pub fn read_path<P: AsRef<Path>>(lflists: &mut IndexMap<String, LFList>, i: P) -> Result<(), Error> {
	let text: String = read_to_string(i)?;
	read(lflists, text)
}

pub fn read(lflists: &mut IndexMap<String, LFList>, text: String) -> Result<(), Error> {
	for text in COMMENTS_REGEX
		.replace_all(&text, "")
		.split("\n!")
		.filter(|i|i.lines().count() > 1) {
		if let Some(key) = text.lines().nth(0) {
			let mut lflist: HashMap<u32, u8> = HashMap::new();
			let mut glist: HashMap<u32, u32> = HashMap::new();
			let mut genesys: u32 = 0;
			let mut hash: u32 = 0x7dfcee6a;
			if let Some(genesy) = text.lines().find(|i| i.starts_with("$genesys")) {
				genesys = {
					let parts: Vec<String> = genesy.split_whitespace().map(String::from).collect();
					if parts.len() > 1 && let Ok(ct) = parts[1].parse::<u32>() {
						hash ^= ((*HASH << 18) | (*HASH >> 14)) ^ ((ct << 9) | (ct >> 23)) ^ ((0x43524544 << 27) | (0x43524544 >> 5));
						ct
					} else { 0 }
				};
			}
			text.lines()
				.filter(|i: &&str| !i.starts_with("#"))
				.filter_map(|i: &str| {
					let i: Vec<String> = i.split_whitespace().map(String::from).collect();
					if i.len() > 1 { Some(i) } else { None }
				})
				.for_each(|i: Vec<String>| {
					if let Ok(code) = i[0].parse::<u32>() {
						if i.len() > 2
							&& i[1].starts_with("$genesys")
							&& let Ok(ct) = i[2].parse::<u32>() {
							glist.insert(code, ct);
							hash ^= ((code << 18) | (code >> 14)) ^ ((*HASH << 9) | (*HASH >> 23)) ^ ((ct << 27) | (ct >> 5));
						} else if let Ok(ct) = i[1].parse::<u8>() {
							lflist.insert(code, ct);
							hash ^= ((code << 18) | (code >> 14)) ^ ((code << (27 + ct)) | (code >> (5 - ct)));
						}
					}
				});
			lflists.insert(String::from(key), LFList {
				lflist: lflist,
				hash: hash,
				glist: glist,
				genesys: genesys
			});
		}
	}
	Ok(())
}
