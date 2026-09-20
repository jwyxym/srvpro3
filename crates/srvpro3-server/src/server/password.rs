use anyhow::{Result, anyhow, bail, ensure};
use ygopro_data::{constants::{MasterRule, Mode, Rule}, message::HostInfo};

pub struct Password {
	pub room_key: Option<String>,
	pub host_info: HostInfo,
	pub best_of: u8,
}

fn lflist(index: i64) -> Result<u32> {
	if index == -1 { return Ok(u32::MAX); }
	let index = usize::try_from(index).map_err(|_| anyhow!("禁限卡表编号无效"))?;
	srvpro3_cards::lflist_by_index(index)
}

fn master_rule(value: u8) -> Result<MasterRule> {
	match value {
		1 => Ok(MasterRule::MasterRule1),
		2 => Ok(MasterRule::MasterRule2),
		3 => Ok(MasterRule::MasterRule3),
		4 => Ok(MasterRule::MasterRuleNew),
		5 => Ok(MasterRule::MasterRule2020),
		_ => bail!("大师规则版本必须为 MR1 到 MR5"),
	}
}

fn number(value: &str, name: &str) -> Result<u32> {
	ensure!(!value.is_empty() && value.bytes().all(|value| value.is_ascii_digit()), "{name}必须为数字");
	value.parse().map_err(Into::into)
}

fn lflist_number(value: &str) -> Result<i64> {
	ensure!(
		!value.is_empty() && (value == "-1" || value.bytes().all(|value| value.is_ascii_digit())),
		"禁限卡表编号必须为非负整数或 -1"
	);
	value.parse().map_err(Into::into)
}

impl Password {
	pub fn parse(pass: &str) -> Result<Self> {
		let (prefix, name) = pass.rsplit_once('#').unwrap_or(("", pass));
		let prefix = prefix.to_uppercase();
		let modes: Vec<&str> = prefix.split([',', '，']).map(str::trim).filter(|mode| !mode.is_empty()).collect();
		let defaults = srvpro3_config::get()?.server.clone();
		let mut host_info = HostInfo {
			mode: Mode::Single,
			lflist: lflist(defaults.lflist)?,
			no_shuffle_deck: !defaults.shuffle,
			duel_rule: master_rule(defaults.master_rule)?,
			draw_count: defaults.draw_count.min(35),
			start_hand: defaults.start_hand.clamp(1, 40),
			time_limit: defaults.time_limit.min(999),
			start_lp: defaults.start_lp.clamp(1, 99_999),
			..HostInfo::default()
		};
		let mut best_of = defaults.bo;
		ensure!(best_of > 0 && best_of < 255 && best_of % 2 == 1, "默认BO局数必须为 1 到 253 的奇数");
		if modes.iter().any(|mode| matches!(*mode, "M" | "MATCH")) { best_of = 3; }

		for mode in &modes {
			match *mode {
				"M" | "MATCH" | "T" | "TAG" => continue,
				"TCG" | "OT" => {
					host_info.rule = Rule::All;
					continue;
				}
				"TCGONLY" | "TO" => {
					host_info.rule = Rule::TCG;
					continue;
				}
				"NOLFLIST" | "NF" => {
					host_info.lflist = lflist(-1)?;
					continue;
				}
				"NOUNIQUE" | "NU" => {
					host_info.rule = Rule::OCG_TCG;
					continue;
				}
				"NOCHECK" | "NC" => {
					host_info.no_check_deck = true;
					continue;
				}
				"NOSHUFFLE" | "NS" => {
					host_info.no_shuffle_deck = true;
					continue;
				}
				_ => {}
			}

			if let Some(value) = mode.strip_prefix("BO") {
				let value = number(value, "BO局数")?;
				ensure!(value > 0 && value < 255 && value % 2 == 1, "BO局数必须为 1 到 253 的奇数");
				best_of = value as u8;
			} else if let Some(value) = mode.strip_prefix("LFLIST").or_else(|| mode.strip_prefix("LF")) {
				host_info.lflist = lflist(lflist_number(value)?)?;
			} else if let Some(value) = mode.strip_prefix("LP") {
				host_info.start_lp = number(value, "生命值")?.clamp(1, 99_999);
			} else if let Some(value) = mode.strip_prefix("TIME").or_else(|| mode.strip_prefix("TM")) {
				let value = number(value, "回合时间")?;
				host_info.time_limit = if value <= 60 { (value * 60) as u16 } else { value.min(999) as u16 };
			} else if let Some(value) = mode.strip_prefix("START").or_else(|| mode.strip_prefix("ST")) {
				host_info.start_hand = number(value, "起手数")?.clamp(1, 40) as u8;
			} else if let Some(value) = mode.strip_prefix("DRAW").or_else(|| mode.strip_prefix("DR")) {
				host_info.draw_count = number(value, "每回合抽卡数")?.min(35) as u8;
			} else if let Some(value) = mode.strip_prefix("MR") {
				host_info.duel_rule = master_rule(number(value, "大师规则版本")? as u8)?;
			}
		}

		let tag = modes.iter().any(|mode| matches!(*mode, "T" | "TAG"));
		if tag {
			if best_of != 1 { bail!("当前引擎不支持双打 BO 多局模式"); }
			host_info.mode = Mode::Tag;
			host_info.start_lp = host_info.start_lp.saturating_mul(2);
		} else if best_of > 1 {
			host_info.mode = Mode::Match;
		}

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
		format!(
			"R#{}-{}-{}-{}-{}-{}-{}-{}-{}-{}#",
			self.host_info.mode as u8,
			self.best_of,
			self.host_info.lflist,
			self.host_info.rule as u8,
			self.host_info.duel_rule as u8,
			self.host_info.start_lp,
			self.host_info.start_hand,
			self.host_info.draw_count,
			self.host_info.time_limit,
			u8::from(self.host_info.no_check_deck) | (u8::from(self.host_info.no_shuffle_deck) << 1),
		)
	}
}
