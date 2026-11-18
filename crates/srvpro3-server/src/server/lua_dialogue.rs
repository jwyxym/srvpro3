use std::{
	any::Any,
	sync::{Arc, atomic::{AtomicUsize, Ordering}},
	time::{Duration, Instant}
};
use mlua::{Function, Lua, Table, Value};
use parking_lot::{Mutex, MutexGuard};
use tokio::sync::mpsc::WeakUnboundedSender;
use ygopro::{
	command::{CommandHandler, COMMANDS},
	duel::{Duel, Request},
	message::DuelInit,
	ygopro_handlers::{HandlerEx, YGOPRO_HANDLERS_EX}
};
use ygopro_derive::{before, command, register_to};

pub const NAME: &str = module_path!();
const MAX_TEXT_BYTES: usize = 4096;
const MAX_PENDING: usize = 16;
const MAX_PER_SECOND: usize = 8;

struct RateLimit {
	start: Instant,
	count: usize,
}

struct DialogueRequest {
	text: String,
	pending: Arc<AtomicUsize>,
}

impl Drop for DialogueRequest {
	fn drop(&mut self) {
		self.pending.fetch_sub(1, Ordering::Relaxed);
	}
}

// 每次 DuelInit 都重新绑定，覆盖单局、比赛换副后的新核心及双打。
// 在 new_card 执行卡片 initial_effect 之前安装接口。
#[before(DuelInit)]
#[register_to(YGOPRO_HANDLERS_EX as HandlerEx)]
fn install(duel: &mut Duel) {
	if let Err(error) = bind(duel) {
		srvpro3_log::error!("注册 Srvpro.Dialogue 失败：{error}");
	}
}

fn enabled() -> bool {
	match srvpro3_config::get() {
		Ok(config) => config.lua_api.dialog,
		Err(_) => false,
	}
}

fn bind(duel: &mut Duel) -> mlua::Result<()> {
	// 只在当前引擎任务同步访问 Lua；Lua 及其派生对象均在本函数返回前释放，
	// 不存入 Rust 对局状态，也不在 core.end() 后保留任何 Lua 句柄。
	let state: Option<Lua> = unsafe { duel.core.lua() };
	let Some(lua) = state else { return Ok(()); };
	let globals: Table = lua.globals();
	let srvpro: Table = match globals.get::<Value>("Srvpro")? {
		Value::Table(table) => table,
		_ => lua.create_table()?,
	};
	let sender: WeakUnboundedSender<Request> = duel.request_sender.downgrade();
	let pending: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
	let limit: Mutex<RateLimit> = Mutex::new(RateLimit { start: Instant::now(), count: 0 });
	let function: Function = lua.create_function(move |_: &Lua, value: Value| -> mlua::Result<bool> {
		// 保留函数以兼容卡片脚本；配置关闭时不入队，也不产生 Lua 错误。
		if !enabled() { return Ok(false); }
		// 严格要求 UTF-8 字符串，不把卡号或其他 Lua 类型隐式转换为文本。
		let Value::String(value) = value else { return Ok(false); };
		if value.as_bytes().len() > MAX_TEXT_BYTES { return Ok(false); }
		let Ok(text) = value.to_str() else { return Ok(false); };
		if text.trim().is_empty() || text.chars().any(|ch: char| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) {
			return Ok(false);
		}
		let Some(sender) = sender.upgrade() else { return Ok(false); };
		let mut rate: MutexGuard<'_, RateLimit> = limit.lock();
		if rate.start.elapsed() >= Duration::from_secs(1) {
			rate.start = Instant::now();
			rate.count = 0;
		}
		if rate.count >= MAX_PER_SECOND || pending.load(Ordering::Relaxed) >= MAX_PENDING { return Ok(false); }
		pending.fetch_add(1, Ordering::Relaxed);
		let request: DialogueRequest = DialogueRequest { text: text.to_string(), pending: pending.clone() };
		// 不捕获 Duel / Lua，不重入核心；仅将纯 Rust 数据排回本房间引擎任务。
		let accepted: bool = sender.send(Request::Command {
			name: "srvpro_lua_dialogue",
			arguments: Some(Box::new(request)),
		}).is_ok();
		if accepted { rate.count += 1; }
		Ok(accepted)
	})?;
	srvpro.set("Dialogue", function)?;
	globals.set("Srvpro", srvpro)?;
	Ok(())
}

#[command]
#[register_to(COMMANDS as CommandHandler with &'static str)]
fn srvpro_lua_dialogue(duel: &mut Duel, arguments: &mut Box<dyn Any + Send>) {
	// 配置重载关闭后，已经排队但尚未发送的台词也不再广播。
	if !enabled() { return; }
	let Some(request) = arguments.downcast_ref::<DialogueRequest>() else { return; };
	super::messages::dialogue_text(duel, &request.text);
}
