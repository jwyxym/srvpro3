use anyhow::Error;
use std::fs::read_to_string;
use tokio::time::{Duration, Instant};
use srvpro3_log::*;

fn cards_reload_interval() -> Result<Duration, Error> {
	Ok(Duration::from_secs(srvpro3_config::get()?.cards.reload.max(0) as u64))
}

pub async fn command() -> Result<(), Error> {
	let mut input = super::input::start()?;
	let mut reload_requests = srvpro3_http::register_reload();
	let mut reload_interval = cards_reload_interval()?;
	let timer = tokio::time::sleep(reload_interval);
	tokio::pin!(timer);
	let mut input_open = true;
	loop {
		let response = tokio::select! {
			biased;
			input = input.receiver.recv(), if input_open => {
				let Some(input) = input else { input_open = false; continue; };
				match input?.trim() {
					"reload" => None,
					"exit" => {
						srvpro3_server::shutdown().await?;
						break;
					}
					_ => continue,
				}
			},
			Some(response) = reload_requests.recv() => Some(response),
			_ = &mut timer, if !reload_interval.is_zero() => {
				if let Err(error) = srvpro3_cards::init().await {
					error!("定时重载卡片失败：{error:#}");
				}
				// 从本次完成时计时，失败也等待一个周期再重试。
				timer.as_mut().reset(Instant::now() + reload_interval);
				continue;
			}
		};
		let result = reload().await;
		// 两种主动重载共用计时：读取最新间隔，从重载完成后重新开始。
		reload_interval = cards_reload_interval()?;
		timer.as_mut().reset(Instant::now() + reload_interval);
		if let Some(response) = response {
			if let Err(error) = &result { error!("HTTP重载失败：{error:#}"); }
			let _ = response.send(result);
		} else {
			result?;
		}
	}
	Ok(())
}

async fn reload() -> Result<(), Error> {
	let text: String = read_to_string("./config.toml")?;
	srvpro3_config::init(&text)?;
	srvpro3_cards::init().await?;
	srvpro3_database::init().await?;
	srvpro3_http::reload().await?;
	srvpro3_server::reload().await?;
	srvpro3_windbot::init()?;
	Ok(())
}
