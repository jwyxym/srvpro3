use std::path::Path;
use anyhow::Error;
use ygopro_cdb_reader::{path, buffer, Card};

pub async fn read<P: AsRef<Path>>(i: P) -> Result<Vec<Card>, Error> {
	Ok(path(i).await?)
}

pub fn read_buffer(i: Vec<u8>) -> Result<Vec<Card>, Error> {
	Ok(buffer(i)?)
}