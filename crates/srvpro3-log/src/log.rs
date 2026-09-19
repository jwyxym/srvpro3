#[macro_export]
macro_rules! srvpro_info {
	($($arg:tt)*) => {
		log::info!(target: "srvpro3", $($arg)*)
	};
}

#[macro_export]
macro_rules! srvpro_warn {
	($($arg:tt)*) => {
		log::warn!(target: "srvpro3", $($arg)*)
	};
}

#[macro_export]
macro_rules! srvpro_error {
	($($arg:tt)*) => {
		log::error!(target: "srvpro3", $($arg)*)
	};
}