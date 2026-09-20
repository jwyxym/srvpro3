use anyhow::Error;
use log::LevelFilter::*;
use simple_logger::SimpleLogger;

pub fn init() -> Result<(), Error> {
	SimpleLogger::new()
		.with_level(Info)
		.with_module_level("ygopro", Off)
		.with_module_level("tokio_kcp", Off)
		.with_module_level("sqlx", Off)
		.init()?;
	Ok(())
}