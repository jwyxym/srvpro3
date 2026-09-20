use anyhow::Error;
use std::fs::read_to_string;

pub async fn init() -> Result<(), Error> {
	srvpro3_log::init()?;
	let text: String = read_to_string("./config.toml")?;
	srvpro3_config::init(&text)?;
	srvpro3_cards::init().await?;
	srvpro3_database::init().await?;
	srvpro3_http::init().await?;
	srvpro3_server::init().await?;
	Ok(())
}