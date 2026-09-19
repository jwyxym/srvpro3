use super::*;
use tokio_util::codec::LengthDelimitedCodec;

pub async fn listen(listener: TcpListener, ready: mpsc::Sender<Connection>) -> Result<()> {
	let mut clients = JoinSet::new();
	loop {
		tokio::select! {
			client = clients.join_next(), if !clients.is_empty() => {
				if let Some(result) = client { report(result); }
			}
			accepted = listener.accept() => {
				let (socket, address) = accepted?;
				if clients.len() >= MAX_CLIENTS { continue; }
				let ready = ready.clone();
				clients.spawn(async move {
					let (read, write) = socket.into_split();
					let input = LengthDelimitedCodec::builder().length_field_type::<u16>()
						.little_endian().new_read(read)
						.map(|frame| frame.map(|bytes| bytes.to_vec()).map_err(anyhow::Error::from));
					let output = LengthDelimitedCodec::builder().length_field_type::<u16>()
						.little_endian().new_write(write)
						.sink_map_err(anyhow::Error::from)
						.with(|bytes: Vec<u8>| futures::future::ready(Ok(bytes.into())));
					session(Box::pin(input), Box::pin(output), ready, None, address.ip()).await
				});
			}
		}
	}
}
