use anyhow::Error;
use indexmap::IndexMap;
use std::{fs::read_to_string, path::Path};

use ygopro_lflist_reader::LFList;

pub fn read_path<P: AsRef<Path>>(lflists: &mut IndexMap<String, LFList>, i: P) -> Result<(), Error> {
	let text: String = read_to_string(i)?;
	read(lflists, text)
}

pub fn read(lflists: &mut IndexMap<String, LFList>, text: String) -> Result<(), Error> {
	let list: IndexMap<String, LFList> = ygopro_lflist_reader::read(&text);
	for i in list {
		lflists.insert(i.0, i.1);
	}
	Ok(())
}