use std::{any::Any, collections::HashMap, str::Chars, sync::LazyLock};
use parking_lot::Mutex;
use tokio::{sync::mpsc::WeakUnboundedSender, task::{Id, try_id}};
use ygopro::{
	command::{CommandHandler, COMMANDS},
	duel::{Duel, Request, SendTarget},
	ygopro_handlers::{Handler, YGOPRO_HANDLERS},
};
use ygopro_core_wrapper::{get_log_message, intptr_t};
use ygopro_data::{constants::Color, message::{ctos, stoc}};
use ygopro_derive::{Attachment, before, command, register_to};
use super::recorder::RecordConfig;

pub const NAME: &str = module_path!();

#[derive(Clone)]
struct Route {
	room_id: String,
	sender: WeakUnboundedSender<Request>,
}

static ROUTES: LazyLock<Mutex<HashMap<Id, Route>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Attachment)]
pub struct Binding {
	task_id: Option<Id>,
}

impl Drop for Binding {
	fn drop(&mut self) {
		if let Some(task_id) = self.task_id.take() { ROUTES.lock().remove(&task_id); }
	}
}

#[before(ctos::CreateGame)]
#[register_to(YGOPRO_HANDLERS)]
fn bind(duel: &mut Duel, config: RecordConfig, binding: &mut Binding) {
	let Some(task_id) = try_id() else { return; };
	let room_id: String = config.0.lock().unwrap().room_id.clone();
	ROUTES.lock().insert(task_id, Route { room_id, sender: duel.request_sender.downgrade() });
	binding.task_id = Some(task_id);
}

// 内核在房间引擎任务中同步回调；使用任务 ID 隔离房间，不依赖运行线程或私有内核指针布局。
pub extern "C" fn report(core: intptr_t, message_type: u32) -> u32 {
	if core == 0 { return 0; }
	let mut buffer: [u8; 1024] = [0; 1024];
	unsafe { get_log_message(core, buffer.as_mut_ptr()); }
	let length: usize = buffer.iter().position(|byte: &u8| *byte == 0).unwrap_or(buffer.len());
	let message: String = String::from_utf8_lossy(&buffer[..length]).into_owned();
	if message.is_empty() { return 0; }
	if message_type != 1 {
		log::debug!("Lua 消息：{message}");
		return 0;
	}
	let route: Option<Route> = try_id().and_then(|task_id: Id| ROUTES.lock().get(&task_id).cloned());
	if let Some(route) = route {
		srvpro3_log::error!("房间 {} Lua 错误：{message}", route.room_id);
		if let Some(sender) = route.sender.upgrade() {
			// 排回原引擎任务发送，避免在内核回调中重入引擎或持有房间状态锁。
			let _ = sender.send(Request::Command { name: "srvpro_lua_error", arguments: Some(Box::new(message)) });
		}
	} else {
		srvpro3_log::error!("Lua 错误（尚未关联房间）：{message}");
	}
	0
}

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_lua_error(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>) {
	let Some(message) = arguments.downcast_ref::<String>() else { return; };
	let mut characters: Chars<'_> = message.chars();
	loop {
		// 每段最多 240 个 UTF-16 单元，为客户端的聊天文本缓冲区留出前缀和结束符。
		let part: String = characters.by_ref().take(120).collect();
		if part.is_empty() { break; }
		duel.sender.send(stoc::Chat { player: Color::Red.into(), msg: format!("[Lua错误] {part}").into() }.into(), SendTarget::All);
	}
}
