use std::{collections::HashMap, path::Path, fs::File, io::Read};
use anyhow::Error;
use parking_lot::Mutex;
use zip::{ZipArchive, read::ZipFile};

use super::{cdb, lflist, CardsSnapshot};

#[derive(Debug)]
pub struct YPK {
	archive: Mutex<ZipArchive<File>>,
	script: HashMap<String, usize>
}

impl YPK {
	pub fn script(&self, name: &str) -> Result<Option<Vec<u8>>, Error> {
		let Some(index) = self.script.get(name) else { return Ok(None) };
		let mut archive = self.archive.lock();
		let mut file = archive.by_index(*index)?;
		let mut content = Vec::new();
		file.read_to_end(&mut content)?;
		Ok(Some(content))
	}
}

pub fn read<P: AsRef<Path>>(snapshot: &mut CardsSnapshot, i: P) -> Result<(), Error> {
	let file: File = File::open(i)?;
	let mut archive: ZipArchive<File> = ZipArchive::new(file)?;
	let len: usize = archive.len();
	let mut script: HashMap<String, usize> = HashMap::new();
	for i in 0..len {
		let mut file: ZipFile<'_> = archive.by_index(i)?;
		if file.is_dir() { continue; }
		let name: String = String::from(file.name());
		if name.starts_with("script/") && name.ends_with(".lua") {
			script.insert(name, i);
		} else if !name.contains('/') {
			if name.ends_with("lflist.conf") {
				let mut content: String = String::new();
				if file.read_to_string(&mut content).is_ok() {
					let _ = lflist::read(&mut snapshot.lflists, content);
				}
			} else if name.ends_with(".cdb") {
				let mut content: Vec<u8> = Vec::new();
				if file.read_to_end(&mut content).is_ok() {
					if let Ok(cards) = cdb::read_buffer(content) {
						for card in cards { snapshot.cards.insert(card.code, card); }
					}
				}
			}
		}
	}
	snapshot.packs.push(YPK { archive: Mutex::new(archive), script });
	Ok(())
}
