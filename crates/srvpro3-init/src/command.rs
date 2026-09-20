use anyhow::Error;
use std::{io::{stdout, stdin, Write}, fs::read_to_string};

pub async fn command() -> Result<(), Error> {
	loop {
		stdout().flush()?;
		let mut input: String = String::new();
		stdin().read_line(&mut input)?;
		match input.trim() {
			"reload" => {
				reload().await?;
			}
			"exit" => {
				srvpro3_server::shutdown().await?;
				break;
			}
			_ => {}
		}
	}
	Ok(())
}

async fn reload() -> Result<(), Error> {
	let text: String = read_to_string("./config.toml")?;
	srvpro3_config::init(&text)?;
	srvpro3_cards::init().await?;
	srvpro3_database::init().await?;
	srvpro3_http::init().await?;
	srvpro3_server::reload().await?;
	srvpro3_windbot::init()?;
	Ok(())
}