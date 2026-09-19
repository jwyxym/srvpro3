pub mod tcp;
pub mod ws;
pub mod udp;

use std::{net::IpAddr, pin::Pin, time::Duration};
use anyhow::{Context, Result, ensure, Error};
use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::{net::TcpListener, sync::mpsc, task::JoinSet, time::timeout};
use tokio_kcp::KcpListener;
use ygopro_data::message::ctos;
use super::handshake::{self, Handshake};

use srvpro3_log::*;

pub type Input = Pin<Box<dyn Stream<Item = Result<Vec<u8>>> + Send>>;
type Output = Pin<Box<dyn Sink<Vec<u8>, Error = anyhow::Error> + Send>>;
const TIMEOUT: Duration = Duration::from_secs(10);
const QUEUE: usize = 64;
const MAX_CLIENTS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
	Tcp,
	Udp,
	Ws,
}

pub struct Connection {
	pub peer_ip: IpAddr,
	pub protocol: Protocol,
	pub handshake: Handshake,
	pub initial: Vec<ctos::Message>,
	pub incoming: mpsc::Receiver<Vec<u8>>,
	pub outgoing: mpsc::Sender<Vec<u8>>,
}

pub struct Listeners {
	tcp: Option<TcpListener>,
	ws: Option<TcpListener>,
	udp: Option<KcpListener>,
}

impl Listeners {
	pub async fn bind(
		tcp_port: u16,
		udp_prot:u16,
		ws_port: u16,
	) -> Result<Self> {
		let tcp: Option<TcpListener> = if tcp_port == 0 {
			warn!("未启用TCP，如需启用，请修改config.toml的内容");
			None
		} else {
			let listener: TcpListener = TcpListener::bind(("0.0.0.0", tcp_port)).await?;
			info!("TCP服务器启动成功 监听在端口：{}", tcp_port);
			Some(listener)
		};
		let ws: Option<TcpListener> = if ws_port == 0 {
			warn!("未启用WS，如需启用，请修改config.toml的内容");
			None
		} else {
			let listener: TcpListener = TcpListener::bind(("0.0.0.0", ws_port)).await?;
			info!("WebSocket服务器启动成功 监听在端口：{}", ws_port);
			Some(listener)
		};
		let udp: Option<KcpListener> = if udp_prot == 0 {
			warn!("未启用UDP，如需启用，请修改config.toml的内容");
			None
		} else {
			let listener: KcpListener = udp::bind(udp_prot).await?;
			info!("UDP服务器启动成功 监听在端口：{}", udp_prot);
			Some(listener)
		};
		Ok(Self { tcp, ws, udp })
	}

	pub fn start(self, ready: mpsc::Sender<Connection>) -> JoinSet<Result<()>> {
		let mut tasks: JoinSet<Result<(), Error>> = JoinSet::new();
		if let Some(tcp) = self.tcp { tasks.spawn(tcp::listen(tcp, ready.clone())); }
		if let Some(ws) = self.ws { tasks.spawn(ws::listen(ws, ready.clone())); }
		if let Some(udp) = self.udp { tasks.spawn(udp::listen(udp, ready)); }
		tasks
	}
}

async fn session(mut input: Input, mut output: Output, ready: mpsc::Sender<Connection>, idle: Option<Duration>, peer_ip: IpAddr, protocol: Protocol) -> Result<()> {
	let (handshake, initial) = timeout(TIMEOUT, handshake::handshake(&mut input)).await
		.context("等待 PlayerInfo 和 JoinGame 超时（10 秒）")?
		.context("业务握手失败")?;
	let (incoming_tx, incoming) = mpsc::channel(QUEUE);
	let (outgoing, mut outgoing_rx) = mpsc::channel(QUEUE);
	timeout(TIMEOUT, ready.send(Connection { peer_ip, protocol, handshake, initial, incoming, outgoing })).await
		.context("提交进房请求超时（10 秒）")?
		.context("房间服务接收通道已关闭")?;
	let result: Result<()> = async {
		let mut accepting_input = true;
		loop {
			tokio::select! {
				frame = async {
					if let Some(duration) = idle {
						timeout(duration, input.next()).await.map_err(Error::from)
					} else {
						Ok(input.next().await)
					}
				}, if accepting_input => {
					let Some(frame) = frame? else { break; };
					match incoming_tx.try_send(frame?) {
						Ok(()) => {}
						Err(mpsc::error::TrySendError::Closed(_)) => accepting_input = false,
						Err(error) => return Err(error.into()),
					}
				}
				frame = outgoing_rx.recv() => {
					let Some(frame) = frame else { break; };
					timeout(TIMEOUT, output.send(frame)).await??;
				}
				_ = incoming_tx.closed(), if accepting_input => accepting_input = false,
			}
		}
		Ok(())
	}.await;
	let _ = timeout(TIMEOUT, output.close()).await;
	result
}

fn packet(body: Vec<u8>) -> Result<Vec<u8>> {
	ensure!(!body.is_empty() && body.len() <= u16::MAX as usize, "无效的消息长度");
	let mut bytes: Vec<u8> = Vec::with_capacity(body.len() + 2);
	bytes.extend_from_slice(&(body.len() as u16).to_le_bytes());
	bytes.extend_from_slice(&body);
	Ok(bytes)
}

fn unpack(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
	let mut frames = Vec::new();
	while !bytes.is_empty() {
		ensure!(bytes.len() >= 2, "缺少消息长度头");
		let len: usize = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
		ensure!(len > 0 && bytes.len() >= len + 2, "业务包不完整");
		frames.push(bytes[2..len + 2].to_vec());
		bytes = &bytes[len + 2..];
	}
	Ok(frames)
}
