use super::*;
use tokio_tungstenite::{accept_async_with_config, tungstenite::{Message, protocol::WebSocketConfig}};

pub async fn listen(listener: TcpListener, ready: mpsc::Sender<Connection>) -> Result<()> {
	let mut clients: JoinSet<Result<(), Error>> = JoinSet::new();
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
					let config = WebSocketConfig::default().max_message_size(Some(1024 * 1024)).max_frame_size(Some(1024 * 1024));
					let socket = timeout(TIMEOUT, accept_async_with_config(socket, Some(config))).await??;
					let (write, read) = socket.split();
					let input = read.take_while(|message| futures::future::ready(!matches!(message, Ok(Message::Close(_)))))
						.flat_map(|message| {
							let frames = match message {
								Ok(Message::Binary(bytes)) => match unpack(&bytes) {
									Ok(frames) => frames.into_iter().map(Ok).collect(),
									Err(error) => vec![Err(error)],
								},
								Ok(Message::Ping(_) | Message::Pong(_)) => Vec::new(),
								Ok(_) => vec![Err(anyhow::anyhow!("WS 只接受二进制业务消息"))],
								Err(error) => vec![Err(error.into())],
							};
							futures::stream::iter(frames)
						});
					let output = write.sink_map_err(anyhow::Error::from).with(|body| {
						futures::future::ready(packet(body).map(|bytes| Message::Binary(bytes.into())))
					});
					session(Box::pin(input), Box::pin(output), ready, None, address.ip(), Protocol::Ws).await
				});
			}
		}
	}
}
