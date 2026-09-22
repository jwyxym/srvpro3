use anyhow::Error;
use std::fs::read_to_string;

pub async fn init() -> Result<(), Error> {
	srvpro3_log::init()?;
	let text: String = read_to_string("./config.toml")?;
	srvpro3_config::init(&text)?;
	// 密钥生成在阻塞线程执行，与资源加载并行；HTTP 监听仍等待鉴权就绪。
	let (auth, resources) = tokio::join!(srvpro3_http::prepare_auth(), async {
		srvpro3_cards::init().await?;
		srvpro3_database::init().await?;
		srvpro3_server::init().await?;
		srvpro3_windbot::init()?;
		Ok::<(), Error>(())
	});
	resources?;
	if let Err(error) = auth {
		srvpro3_log::warn!("HTTP鉴权预初始化失败，将在启动HTTP时重试：{error:#}");
	}
	srvpro3_http::init().await?;
	super::command::command().await?;
	Ok(())
}
