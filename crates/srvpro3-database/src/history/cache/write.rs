#[macro_export]
macro_rules! write {
	($key:expr, $value:expr) => {{
		use crate::connect;
		use serde_json::to_string;
		use redis::{RedisError, AsyncCommands};
		if let Ok(mut connection) = connect::get::redis() {
			if let Ok(value) = to_string(&$value) {
				let _: Result<(), RedisError> =
					AsyncCommands::set_ex(&mut connection, &$key, value, 3600).await;
			}
		}
	}};
}

pub use write;