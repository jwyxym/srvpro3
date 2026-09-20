use std::{
	ffi::CString,
	sync::{LazyLock, Mutex, MutexGuard},
	thread::{self, JoinHandle},
	path::PathBuf
};
use anyhow::{Context, Error, ensure};
use libloading::Library;

use srvpro3_log::*;

use super::{Start, Stop};

struct Server {
	port: u16,
	stop: Stop,
	task: JoinHandle<()>
}

static SERVER: LazyLock<Mutex<Option<Server>>> = LazyLock::new(|| Mutex::new(None));

pub fn init() -> Result<(), Error> {
	reload()
}

pub fn reload() -> Result<(), Error> {
	let config = srvpro3_config::get()?;
	let path: String = config.windbot.path.clone();
	let port: u16 = config.windbot.port;
	let mut server: MutexGuard<'_, Option<Server>> = SERVER.lock()
		.map_err(|_| anyhow::anyhow!("获取WindBot server状态锁错误"))?;
	if path.trim().is_empty() || port == 0 {
		if let Some(server) = server.take() { stop(server)?; }
		warn!("未启用 WindBot，如需启用，请设置 config.toml 的 windbot.path");
		return Ok(());
	}
	if server.as_ref()
		.is_some_and(|server| server.port == port && !server.task.is_finished()) {
		return Ok(());
	}
	if let Some(server) = server.take() { stop(server)?; }
	*server = Some(start(&path, port)?);
	Ok(())
}

fn start(path: &str, port: u16) -> Result<Server, Error> {
	let sqlite_path: String = path.to_string();
	let path: PathBuf = PathBuf::from(path);
	let library_path: PathBuf = {
		#[cfg(target_os = "windows")]
		let path = path.join("WindBot.dll");
		#[cfg(target_os = "linux")]
		let path = path.join("WindBot.so");
		#[cfg(target_os = "macos")]
		let path = path.join("WindBot.dylib");
		path
	};
	let db_path: String = path.join("cards.cdb")
		.to_string_lossy()
		.replace('"', "\\\"");

	let library: &mut Library = Box::leak(
		Box::new(unsafe {
			Library::new(&library_path)
		}
			.with_context(|| format!("加载 WindBot 动态库失败：{}", library_path.display()))?
		)
	);
	let start: Start = *unsafe {
		library.get::<Start>(b"windbot_start\0")
	}
		.context("WindBot 动态库缺少 windbot_start")?;

	let stop: Stop = *unsafe {
		library.get::<Stop>(b"windbot_stop\0")
	}
		.context("WindBot 动态库缺少 windbot_stop")?;

	let task: JoinHandle<()> = thread::Builder::new()
		.name("windbot-server".into()).spawn(move || {
		let arguments: CString = match CString::new(
			format!(
				"ServerMode=true ServerPort={port} ServerURL=127.0.0.1 SQLitePath=\"{}\" DbPath=\"{}\" ConsoleLog=false",
				sqlite_path, db_path
			)
		) {
			Ok(arguments) => arguments,
			Err(error) => {
				error!("启动 WindBot 失败：{error}");
				return;
			}
		};
		let status: i32 = unsafe { start(arguments.as_ptr()) };
		if status != 0 { error!("WindBot server 已退出，返回值：{status}"); }
	})
		.context("创建 WindBot server 线程失败")?;
	info!("WindBot服务器启动成功 监听在端口：{}", port);
	Ok(Server { port, stop, task })
}

fn stop(server: Server) -> Result<(), Error> {
	let status: i32 = unsafe { (server.stop)() };
	ensure!(status == 0, "停止 WindBot server 失败，返回值：{status}");
	server.task.join().map_err(|_| anyhow::anyhow!("等待 WindBot server 退出失败"))
}
