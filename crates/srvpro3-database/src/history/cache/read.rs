#[macro_export]
macro_rules! read {
	($key:expr, $type:ty) => {{
		use crate::connect;
		use redis::AsyncCommands;

		let mut result: Option<$type> = None;
		if let Ok(mut connection) = connect::get::redis() {
			if let Ok(value) = AsyncCommands::get::<_, String>(&mut connection, &$key).await {
				result = serde_json::from_str::<$type>(&value).ok();
			}
		}
		result
	}};
}

pub use read;
