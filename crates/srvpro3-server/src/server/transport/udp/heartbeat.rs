use super::super::{Output, TIMEOUT};
use anyhow::{Result, bail, ensure};
use futures::SinkExt;
use tokio::time::{Duration, Instant, timeout};

// UDP 扩展消息，两个方向使用相同编号；消息体只有一个 opcode。
pub const CTOS_PING: u8 = 0xF0;
pub const CTOS_PONG: u8 = 0xF1;
pub const STOC_PING: u8 = 0xF0;
pub const STOC_PONG: u8 = 0xF1;
pub const INTERVAL: Duration = Duration::from_secs(5);
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub const CLIENT_PING_TIMEOUT: Duration = Duration::from_secs(20);

enum Phase {
	WaitPong,
	WaitPing,
	SendPing,
}

pub struct Heartbeat {
	phase: Phase,
	pub deadline: Instant,
}

impl Heartbeat {
	pub async fn start(output: &mut Output) -> Result<Self> {
		timeout(TIMEOUT, output.send(vec![STOC_PING])).await??;
		Ok(Self { phase: Phase::WaitPong, deadline: Instant::now() + RESPONSE_TIMEOUT })
	}

	pub async fn receive(&mut self, frame: &[u8], output: &mut Output) -> Result<bool> {
		let Some(&opcode) = frame.first() else { return Ok(false) };
		if !matches!(opcode, CTOS_PING | CTOS_PONG) { return Ok(false); }
		ensure!(frame.len() == 1, "UDP心跳消息不能携带额外数据");
		if opcode == CTOS_PONG {
			if matches!(self.phase, Phase::WaitPong) {
				self.phase = Phase::WaitPing;
				self.deadline = Instant::now() + CLIENT_PING_TIMEOUT;
			}
		} else {
			timeout(TIMEOUT, output.send(vec![STOC_PONG])).await??;
			if matches!(self.phase, Phase::WaitPing) {
				self.phase = Phase::SendPing;
				self.deadline = Instant::now() + INTERVAL;
			}
		}
		// 额外 PING 仍会回复，但重复或乱序心跳不能延长当前等待期限。
		Ok(true)
	}

	pub async fn tick(&mut self, output: &mut Output) -> Result<()> {
		match self.phase {
			Phase::SendPing => {
				timeout(TIMEOUT, output.send(vec![STOC_PING])).await??;
				self.phase = Phase::WaitPong;
				self.deadline = Instant::now() + RESPONSE_TIMEOUT;
			}
			Phase::WaitPong => bail!("UDP心跳超时：未收到CTOS.PONG"),
			Phase::WaitPing => bail!("UDP心跳超时：未收到CTOS.PING"),
		}
		Ok(())
	}
}
