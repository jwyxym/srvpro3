use std::io::{stdin, stdout, IsTerminal, Write};
use anyhow::Error;
use rustyline_async::{Readline, ReadlineEvent};
use tokio::{sync::mpsc, task::JoinHandle};

struct Terminal(Readline);

impl Drop for Terminal {
	fn drop(&mut self) {
		srvpro3_log::set_writer(None);
		let _ = self.0.flush();
	}
}

pub struct Input {
	pub receiver: mpsc::Receiver<Result<String, Error>>,
	task: Option<JoinHandle<()>>,
}

impl Drop for Input {
	fn drop(&mut self) {
		if let Some(task) = self.task.take() { task.abort(); }
	}
}

pub fn start() -> Result<Input, Error> {
	let (sender, receiver) = mpsc::channel(16);
	if stdin().is_terminal() && stdout().is_terminal() {
		let (readline, writer) = Readline::new("> ".into())?;
		srvpro3_log::set_writer(Some(Box::new(writer)));
		let mut terminal = Terminal(readline);
		let task = tokio::spawn(async move {
			loop {
				let line = match terminal.0.readline().await {
					Ok(ReadlineEvent::Line(line)) => {
						if !line.trim().is_empty() { terminal.0.add_history_entry(line.clone()); }
						line
					}
					Ok(ReadlineEvent::Eof | ReadlineEvent::Interrupted) => {
						let _ = sender.send(Ok("exit".into())).await;
						break;
					}
					Err(error) => { let _ = sender.send(Err(error.into())).await; break; }
				};
				let exiting = line.trim() == "exit";
				if sender.send(Ok(line)).await.is_err() || exiting { break; }
			}
		});
		Ok(Input { receiver, task: Some(task) })
	} else {
		// 管道或无终端环境不启用终端控制，也不占用异步运行时的线程。
		std::thread::Builder::new().name("srvpro3-input".into()).spawn(move || {
			loop {
				let result = (|| -> std::io::Result<Option<String>> {
					stdout().flush()?;
					let mut line = String::new();
					if stdin().read_line(&mut line)? == 0 { return Ok(None); }
					Ok(Some(line))
				})();
				match result {
					Ok(Some(line)) => if sender.blocking_send(Ok(line)).is_err() { break; },
					Ok(None) => break,
					Err(error) => { let _ = sender.blocking_send(Err(error.into())); break; }
				}
			}
		})?;
		Ok(Input { receiver, task: None })
	}
}
