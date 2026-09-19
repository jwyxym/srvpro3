#[macro_export]
macro_rules! clear {
	($pattern:expr) => {{
		use crate::connect;
		use redis::AsyncCommands;

		if let Ok(mut connection) = connect::get::redis() {
			if let Ok(keys) = AsyncCommands::keys::<_, Vec<String>>(&mut connection, $pattern).await {
				if !keys.is_empty() {
					let _: Result<(), redis::RedisError> =
						AsyncCommands::del(&mut connection, keys).await;
				}
			}
		}
	}};
}

pub use clear;