use std::sync::LazyLock;
use axum::{Json, http::StatusCode};
use parking_lot::{Mutex, RwLockReadGuard};
use serde::Serialize;
use sysinfo::{get_current_pid, ProcessRefreshKind, ProcessesToUpdate, System};

use srvpro3_config::Config;

static SYSTEM: LazyLock<Mutex<System>> = LazyLock::new(|| Mutex::new(System::new_all()));

#[derive(Serialize)]
pub struct HostInfo {
	pub system: SystemInfo,
	pub cpu: CpuInfo,
	pub memory: MemoryInfo,
}

#[derive(Serialize)]
pub struct SystemInfo {
	pub version: String,
}

#[derive(Serialize)]
pub struct CpuInfo {
	pub model: String,
	pub cores: usize,
	pub frequency_mhz: u64,
	pub usage_percent: f32,
	pub srvpro3_usage_percent: f32,
}

#[derive(Serialize)]
pub struct MemoryInfo {
	pub total_bytes: u64,
	pub total_mb: f64,
	pub used_bytes: u64,
	pub usage_percent: f32,
}

#[derive(Serialize)]
pub struct Usage {
	pub cpu_usage_percent: f32,
	pub srvpro3_cpu_usage_percent: f32,
	pub memory_usage_percent: f32,
}

fn check() -> Result<(), (StatusCode, &'static str)> {
	let config: RwLockReadGuard<'_, Config> = srvpro3_config::get()
		.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "配置尚未加载"))?;
	if config.http_api.host { Ok(()) } else { Err((StatusCode::NOT_FOUND, "主机信息接口未启用")) }
}

pub fn info() -> HostInfo {
	let mut system = SYSTEM.lock();
	system.refresh_cpu_all();
	system.refresh_memory();
	let srvpro3_usage_percent = get_current_pid().ok().map(|pid| {
		system.refresh_processes_specifics(
			ProcessesToUpdate::Some(&[pid]),
			true,
			ProcessRefreshKind::nothing().with_cpu(),
		);
		system.process(pid).map_or(0.0, |process| process.cpu_usage())
	}).unwrap_or(0.0);
	let cpu = system.cpus().first();
	let total_bytes = system.total_memory();
	let used_bytes = system.used_memory();
	let name = System::name().unwrap_or_else(|| "未知系统".into());
	let kernel = System::kernel_version().unwrap_or_else(|| "未知版本".into());
	HostInfo {
		system: SystemInfo { version: format!("{name} {} {kernel}", std::env::consts::ARCH) },
		cpu: CpuInfo {
			model: cpu.map_or_else(|| "未知CPU".into(), |cpu| cpu.brand().to_owned()),
			cores: system.cpus().len(),
			frequency_mhz: cpu.map_or(0, |cpu| cpu.frequency()),
			usage_percent: system.global_cpu_usage(),
			srvpro3_usage_percent,
		},
		memory: MemoryInfo {
			total_bytes,
			total_mb: total_bytes as f64 / 1024.0 / 1024.0,
			used_bytes,
			usage_percent: if total_bytes == 0 { 0.0 } else { used_bytes as f32 / total_bytes as f32 * 100.0 },
		},
	}
}

pub fn usage() -> Usage {
	let info = info();
	Usage {
		cpu_usage_percent: info.cpu.usage_percent,
		srvpro3_cpu_usage_percent: info.cpu.srvpro3_usage_percent,
		memory_usage_percent: info.memory.usage_percent,
	}
}

pub async fn get() -> Result<Json<HostInfo>, (StatusCode, &'static str)> {
	check()?;
	Ok(Json(info()))
}
