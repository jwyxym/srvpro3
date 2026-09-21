pub mod heartbeat;

use super::*;
use tokio_kcp::KcpConfig;
use tokio_util::codec::LengthDelimitedCodec;

pub async fn bind(port: u16) -> Result<KcpListener> {
	let config = KcpConfig {
		mtu: 1200,
		stream: true,
		session_expire: Duration::from_secs(300),
		..Default::default()
	};
	Ok(KcpListener::bind(config, ("0.0.0.0", port)).await?)
}

// UDP 只承载 KCP 数据，重传、排序和分段重组由 KCP 库处理。
pub async fn listen(mut listener: KcpListener, ready: mpsc::Sender<Connection>) -> Result<()> {
	let mut clients = JoinSet::new();
	loop {
		tokio::select! {
			client = clients.join_next(), if !clients.is_empty() => {
				let _ = client;
			}
			accepted = listener.accept() => {
				let (socket, address) = accepted?;
				if clients.len() >= MAX_CLIENTS { continue; }
				let ready = ready.clone();
				clients.spawn(async move {
					let (read, write) = tokio::io::split(socket);
					// KCP 还原为字节流后，再按原有 TCP 格式读取业务包。
					let input = LengthDelimitedCodec::builder().length_field_type::<u16>()
						.little_endian().new_read(read)
						.map(|frame| frame.map(|bytes| bytes.to_vec()).map_err(anyhow::Error::from));
					let output = LengthDelimitedCodec::builder().length_field_type::<u16>()
						.little_endian().new_write(write)
						.sink_map_err(anyhow::Error::from)
						.with(|bytes: Vec<u8>| futures::future::ready(Ok(bytes.into())));
					session(Box::pin(input), Box::pin(output), ready, None, address.ip(), Protocol::Udp).await
				});
			}
		}
	}
}
