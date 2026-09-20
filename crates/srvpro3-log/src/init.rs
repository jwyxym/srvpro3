use anyhow::Error;
use log::LevelFilter::*;
use simple_logger::SimpleLogger;
use std::{io::Write, sync::Mutex};

static WRITER: Mutex<Option<Box<dyn Write + Send>>> = Mutex::new(None);

/// 交互终端接管日志输出；传入 None 恢复普通控制台输出。
pub fn set_writer(writer: Option<Box<dyn Write + Send>>) {
	*WRITER.lock().unwrap() = writer;
}

struct Logger(SimpleLogger);

impl log::Log for Logger {
	fn enabled(&self, metadata: &log::Metadata<'_>) -> bool { log::Log::enabled(&self.0, metadata) }

	fn log(&self, record: &log::Record<'_>) {
		if !self.enabled(record.metadata()) { return; }
		let mut writer = WRITER.lock().unwrap();
		if let Some(writer) = writer.as_mut() {
			let _ = writeln!(writer, "{} {:5} [{}] {}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true), record.level(), record.target(), record.args());
		} else {
			log::Log::log(&self.0, record);
		}
	}

	fn flush(&self) {
		if let Some(writer) = WRITER.lock().unwrap().as_mut() { let _ = writer.flush(); }
	}
}

pub fn init() -> Result<(), Error> {
	let logger = SimpleLogger::new()
		.with_level(Info)
		.with_module_level("ygopro", Off)
		.with_module_level("tokio_kcp", Off)
		.with_module_level("sqlx", Off);
	log::set_boxed_logger(Box::new(Logger(logger)))?;
	log::set_max_level(Info);
	Ok(())
}
