use anyhow::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
	srvpro3_init::init().await?;
	Ok(())
}