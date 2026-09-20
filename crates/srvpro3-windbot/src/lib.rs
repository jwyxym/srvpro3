mod init;
mod add;
mod bot;

pub use init::{init, reload};
pub use add::add;
pub use bot::Bot;
type Start = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;
type Stop = unsafe extern "C" fn() -> i32;
