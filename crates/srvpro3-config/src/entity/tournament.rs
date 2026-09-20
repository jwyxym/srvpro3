use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Tournament {
	pub enabled: bool,
	pub url: String,
	pub token: String,
	pub tournament_id: u64,
	pub bo: u8,
	pub check_deck: bool,
	pub post_score: bool,
	/// 排表请求和比分回传的超时时间，单位为秒。
	pub timeout: u64,
}

impl Default for Tournament {
	fn default() -> Self {
		Self {
			enabled: false,
			url: "https://api-tabulator.moecube.com:444/api/srvpro".into(),
			token: String::new(),
			tournament_id: 0,
			bo: 3,
			check_deck: true,
			post_score: true,
			timeout: 5,
		}
	}
}

impl std::fmt::Debug for Tournament {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("Tournament")
			.field("enabled", &self.enabled)
			.field("tournament_id", &self.tournament_id)
			.field("bo", &self.bo)
			.field("check_deck", &self.check_deck)
			.field("post_score", &self.post_score)
			.finish_non_exhaustive()
	}
}
