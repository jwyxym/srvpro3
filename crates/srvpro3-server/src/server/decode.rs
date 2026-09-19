use binrw::BinRead;
use std::io::Cursor;
use ygopro_data::message::ctos::Message;

pub fn decode(frame: &[u8]) -> Option<Message> {
	let mut cursor: Cursor<&[u8]> = Cursor::new(frame);
	BinRead::read_le(&mut cursor).ok()
}