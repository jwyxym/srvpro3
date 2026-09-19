use anyhow::{Result, bail, ensure};
use ygopro_data::{constants::Mode, message::HostInfo};

/// 解析 JoinGame.pass 中的模式前缀；原始 pass 仍用于历史记录。
pub struct Password {
	pub room_key: Option<String>,
	pub host_info: HostInfo,
	pub best_of: u8,
}

impl Password {
	pub fn parse(pass: &str) -> Result<Self> {
		let (prefix, name) = pass.rsplit_once('#').unwrap_or(("", pass));
		let prefix = prefix.to_uppercase();
		let modes: Vec<&str> = prefix.split([',', '，']).collect();
		// srvpro2 无模式前缀时采用单局；ygopro 的默认值是 Match，需显式覆盖。
		let mut host_info = HostInfo { mode: Mode::Single, ..HostInfo::default() };
		let mut best_of = if modes.iter().any(|mode| matches!(*mode, "M" | "MATCH")) { 3 } else { 1 };
		for mode in &modes {
			if let Some(number) = mode.strip_prefix("BO") {
				if !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()) {
					best_of = number.parse().map_err(|_| anyhow::anyhow!("BO 局数必须为 1 到 253 的奇数"))?;
					// 引擎插件使用 u8 计算 best_of + 1，不能传入 255。
					ensure!(best_of > 0 && best_of < 255 && best_of % 2 == 1, "BO 局数必须为 1 到 253 的奇数");
					break;
				}
			}
		}
		let tag = modes.iter().any(|mode| matches!(*mode, "T" | "TAG"));
		if tag {
			if best_of != 1 { bail!("当前引擎不支持双打 BO 多局模式"); }
			host_info.mode = Mode::Tag;
			host_info.start_lp *= 2;
		} else if best_of > 1 {
			host_info.mode = Mode::Match;
		}
		// 保留完整房间名，避免 M#123 和 T#123 加入同一个房间。
		let room_key = if name.is_empty() {
			None
		} else if pass.contains('#') {
			Some(format!("{prefix}#{name}"))
		} else {
			Some(name.to_owned())
		};
		Ok(Self { room_key, host_info, best_of })
	}

	pub fn configure(&self, configuration: &mut ygopro::Configuration) {
		if self.host_info.mode != Mode::Tag && self.best_of > 3 {
			configuration.enable_plugin_with_configuration(
				ygopro::plugin::bo::NAME,
				ygopro::plugin::bo::Configuration { override_best_of: self.best_of },
			);
		}
	}

	pub fn capacity(&self) -> usize {
		if self.host_info.mode == Mode::Tag { 4 } else { 2 }
	}

	pub fn random_prefix(&self) -> String {
		format!("R#{}-{}#", self.host_info.mode as u8, self.best_of)
	}
}
