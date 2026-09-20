use std::{sync::LazyLock, time::Duration};
use anyhow::{Result, anyhow, ensure};
use reqwest::{Client, Response, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use srvpro3_config::Tournament as Config;

static CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(|| Client::builder()
	.redirect(reqwest::redirect::Policy::none()).build());

#[derive(Debug)]
pub struct Api {
	pub config: Config,
	endpoint: Url,
}

#[derive(Deserialize)]
struct Envelope { tournament: Tournament }

#[derive(Deserialize)]
pub struct Tournament {
	pub id: u64,
	pub participants: Vec<ParticipantEntry>,
	pub matches: Vec<MatchEntry>,
}

#[derive(Deserialize)]
pub struct ParticipantEntry { pub participant: Participant }

#[derive(Clone, Debug, Deserialize)]
pub struct Participant {
	pub id: u64,
	pub name: String,
	#[serde(default)]
	pub quit: bool,
	pub deckbuf: Option<String>,
}

#[derive(Deserialize)]
pub struct MatchEntry {
	#[serde(rename = "match")]
	pub match_info: Match,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Match {
	pub id: u64,
	pub state: String,
	pub player1_id: Option<u64>,
	pub player2_id: Option<u64>,
	pub winner_id: Option<Winner>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Winner { Player(u64), Tie(String) }

#[derive(Clone, Debug, Serialize)]
pub struct Score {
	pub scores_csv: String,
	pub winner_id: Winner,
}

#[derive(Deserialize)]
struct ScoreResult { success: bool }

impl Api {
	pub fn report_key(&self, match_id: u64) -> String {
		format!("{}#{match_id}", self.endpoint)
	}

	pub fn new(config: Config) -> Result<Self> {
		let mut endpoint = Url::parse(config.url.trim()).map_err(|_| anyhow!("赛事接口地址无效"))?;
		ensure!(matches!(endpoint.scheme(), "http" | "https") && endpoint.host_str().is_some()
			&& endpoint.username().is_empty() && endpoint.password().is_none()
			&& endpoint.query().is_none() && endpoint.fragment().is_none(), "赛事接口地址只能包含 HTTP/HTTPS 地址及路径，token 请单独填写");
		endpoint.set_path(&format!("{}/v1/tournaments/{}", endpoint.path().trim_end_matches('/'), config.tournament_id));
		Ok(Self { config, endpoint })
	}

	fn client(&self) -> Result<&Client> {
		CLIENT.as_ref().map_err(|_| anyhow!("创建赛事接口客户端失败"))
	}

	pub async fn get(&self) -> Result<Tournament> {
		let mut url = self.endpoint.clone();
		url.set_path(&format!("{}.json", url.path()));
		let response = self.client()?.get(url)
			.query(&[("api_key", self.config.token.as_str()), ("include_participants", "1"), ("include_matches", "1")])
			.timeout(Duration::from_secs(self.config.timeout)).send().await
			.map_err(|error| anyhow!("读取比赛排表失败：{}", error.without_url()))?;
		Ok(read_json::<Envelope>(response).await?.tournament)
	}

	async fn put_score(&self, match_id: u64, score: &Score) -> Result<()> {
		let mut url = self.endpoint.clone();
		url.set_path(&format!("{}/matches/{match_id}.json", url.path()));
		let response = self.client()?.put(url)
			.json(&serde_json::json!({ "api_key": self.config.token, "match": score }))
			.timeout(Duration::from_secs(self.config.timeout)).send().await
			.map_err(|error| anyhow!("上传比赛成绩失败：{}", error.without_url()))?;
		ensure!(read_json::<ScoreResult>(response).await?.success, "排表服务拒绝了比赛成绩");
		Ok(())
	}

	pub async fn report(&self, match_id: u64, score: &Score) -> Result<()> {
		for attempt in 0..3 {
			match self.put_score(match_id, score).await {
				Ok(()) => return Ok(()),
				Err(error) if attempt == 2 => return Err(error),
				Err(_) => tokio::time::sleep(Duration::from_secs(1 << attempt)).await,
			}
		}
		unreachable!()
	}
}

async fn read_json<T: DeserializeOwned>(mut response: Response) -> Result<T> {
	ensure!(response.status().is_success(), "赛事接口返回 HTTP {}", response.status().as_u16());
	// 不把响应正文或含 api_key 的请求 URL 放入错误日志。
	let mut body = Vec::new();
	while let Some(chunk) = response.chunk().await.map_err(|_| anyhow!("读取赛事接口响应失败"))? {
		ensure!(body.len() + chunk.len() <= 4 * 1024 * 1024, "赛事接口响应过大");
		body.extend_from_slice(&chunk);
	}
	serde_json::from_slice(&body).map_err(|_| anyhow!("赛事接口返回的数据格式无效"))
}
