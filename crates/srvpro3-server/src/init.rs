use super::server::Server;

use anyhow::Error;
use parking_lot::{RawRwLock, lock_api::RwLockReadGuard};

use srvpro3_config::Config;

pub async fn init() -> Result<(), Error> {
	ygopro::init();
	let config: RwLockReadGuard<'_, RawRwLock, Config> = srvpro3_config::get()?;
	let tcp_port: u16 = config.server.tcp.port;
	let udp_port: u16 = config.server.udp.port;
	let ws_port: u16 = config.server.ws.port;
	let reconnect_timeout: u64 = config.server.reconnect_timeout_secs;
	let side_timeout: u64 = config.server.side_timeout_secs;
	drop(config);
	let server: Server = Server::bind(
		tcp_port,
		udp_port,
		ws_port,
		reconnect_timeout,
		side_timeout
	).await?;
	server.run().await
}
