mod init;
mod add;
mod bot;
mod list;

pub use init::{init, reload};
pub use add::add;
pub use bot::Bot;
pub use list::{BotInfo, random};
type Start = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;
type Stop = unsafe extern "C" fn() -> i32;
