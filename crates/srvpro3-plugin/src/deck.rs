mod component;

use std::{fs::File, io::Read, sync::{Arc, LazyLock, Mutex, MutexGuard}};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use srvpro3_deck::Deck as DeckData;
use srvpro3_cards::CardsSnapshot;
use ygopro_cdb_reader::Card;
use wasmtime::{Config, Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc, error::Context as WasmContext};
use wasmtime_wasi::{WasiCtxBuilder, p1::{self, WasiP1Ctx}};

const FUEL: u64 = 10_000_000;
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 64 * 1024;
const INPUT_LIMIT: usize = 4 * 1024 * 1024;
const MODULE_LIMIT: usize = 64 * 1024 * 1024;

struct Compiled { hash: Vec<u8>, engine: Engine, module: Module }
struct State { limits: StoreLimits, wasi: WasiP1Ctx }
type ModuleCache = Option<Arc<Compiled>>;
static MODULE: LazyLock<Mutex<ModuleCache>> = LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize, Serialize)]
pub struct Classification { pub id: String, pub name: String }

#[derive(Serialize)]
struct CardInput {
	code: u32,
	alias: u32,
	setcode: [u16; 16],
	card_type: u32,
	level: u32,
	attribute: u32,
	race: u32,
	attack: i32,
	defense: i32,
	lscale: u32,
	rscale: u32,
	link_marker: u32,
	ot: u8,
	category: u32,
}

impl From<&Card> for CardInput {
	fn from(card: &Card) -> Self {
		Self {
			code: card.code, alias: card.alias, setcode: card.setcode,
			card_type: card.card_type, level: card.level, attribute: card.attribute,
			race: card.race, attack: card.attack, defense: card.defense,
			lscale: card.lscale, rscale: card.rscale, link_marker: card.link_marker,
			ot: card.ot, category: card.category,
		}
	}
}

#[derive(Serialize)]
#[serde(untagged)]
enum InputCard {
	Known(CardInput),
	Missing { code: u32, missing: bool },
}

#[derive(Serialize)]
struct Input {
	main: Vec<InputCard>,
	extra: Vec<InputCard>,
	side: Vec<InputCard>,
}

impl Input {
	fn section(codes: &[u32], cards: &CardsSnapshot) -> Vec<InputCard> {
		codes.iter().map(|&code: &u32| match cards.cards.get(&code) {
			Some(card) => InputCard::Known(CardInput::from(card)),
			None => InputCard::Missing { code, missing: true },
		}).collect()
	}
}

pub struct Deck { runtime: Runtime }

enum Runtime { Core(CoreDeck), Component(component::ComponentDeck) }

impl Deck {
	pub fn load(path: &str) -> Result<Self> {
		let file: File = File::open(path).context("读取分类 WASM 文件失败")?;
		ensure!(file.metadata()?.is_file(), "分类 WASM 必须为普通文件");
		let mut bytes: Vec<u8> = Vec::new();
		file.take(MODULE_LIMIT as u64 + 1).read_to_end(&mut bytes)?;
		Self::from_bytes(bytes)
	}

	fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
		ensure!(bytes.len() <= MODULE_LIMIT, "分类 WASM 超过 64 MiB");
		let runtime: Runtime = match bytes.get(..8) {
			Some([0, 97, 115, 109, 1, 0, 0, 0]) => Runtime::Core(CoreDeck::from_bytes(bytes)?),
			Some([0, 97, 115, 109, 13, 0, 1, 0]) => Runtime::Component(component::ComponentDeck::from_bytes(bytes)?),
			_ => anyhow::bail!("分类 WASM 文件格式无效"),
		};
		Ok(Self { runtime })
	}

	pub fn format(&self) -> &'static str {
		match &self.runtime { Runtime::Core(_) => "core", Runtime::Component(_) => "component" }
	}

	/// 返回与分类调用相同卡库快照构造的输入，供管理员检查插件收到的数据。
	pub fn input(&self, deck: &DeckData) -> Result<Value> {
		deck.validate()?;
		let cards: &CardsSnapshot = match &self.runtime {
			Runtime::Core(runtime) => &runtime.cards,
			Runtime::Component(runtime) => runtime.cards(),
		};
		serde_json::to_value(Input {
			main: Input::section(&deck.main, cards),
			extra: Input::section(&deck.extra, cards),
			side: Input::section(&deck.side, cards),
		}).map_err(Into::into)
	}

	pub fn classify(&mut self, deck: &DeckData) -> Result<Classification> {
		match &mut self.runtime {
			Runtime::Core(runtime) => runtime.classify(deck),
			Runtime::Component(runtime) => runtime.classify(deck),
		}
	}
}

struct CoreDeck {
	cards: Arc<CardsSnapshot>,
	store: Store<State>,
	memory: Memory,
	allocate: TypedFunc<i32, i32>,
	release: TypedFunc<(i32, i32), ()>,
	classify: TypedFunc<(i32, i32), i64>,
}

impl CoreDeck {
	/// 相同内容复用编译结果；每次查询创建独立实例，不保留对其他查询可见的模块状态。
	fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
		ensure!(bytes.len() <= 8 * 1024 * 1024 && bytes.starts_with(&[0, 97, 115, 109, 1, 0, 0, 0]), "分类 WASM 无效或超过 8 MiB");
		let hash: Vec<u8> = Sha256::digest(&bytes).to_vec();
		let compiled: Arc<Compiled> = {
			let mut cache: MutexGuard<'_, ModuleCache> = MODULE.lock().map_err(|_| anyhow::anyhow!("分类模块缓存不可用"))?;
			if let Some(compiled) = cache.as_ref().filter(|compiled| compiled.hash == hash) { compiled.clone() }
			else {
				let mut config: Config = Config::new();
				config.consume_fuel(true).wasm_exceptions(true);
				let engine: Engine = Engine::new(&config)?;
				let module: Module = Module::new(&engine, bytes).context("编译分类 WASM 失败")?;
				ensure!(module.imports().all(|import| import.module() == "wasi_snapshot_preview1"), "分类 WASM 仅支持 wasi_snapshot_preview1 宿主导入");
				let compiled: Arc<Compiled> = Arc::new(Compiled { hash, engine, module });
				*cache = Some(compiled.clone());
				compiled
			}
		};
		let limits: StoreLimits = StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).instances(1).memories(1).tables(1).table_elements(10000).trap_on_grow_failure(true).build();
		let wasi: WasiP1Ctx = WasiCtxBuilder::new().build_p1();
		let mut store: Store<State> = Store::new(&compiled.engine, State { limits, wasi });
		store.limiter(|state: &mut State| &mut state.limits);
		store.set_fuel(FUEL)?;
		let mut linker: Linker<State> = Linker::new(&compiled.engine);
		p1::add_to_linker_sync(&mut linker, |state: &mut State| &mut state.wasi)?;
		let instance: Instance = linker.instantiate(&mut store, &compiled.module).context("初始化分类 WASM 失败")?;
		let memory: Memory = instance.get_memory(&mut store, "memory").context("WASM 缺少 memory 导出")?;
		let allocate: TypedFunc<i32, i32> = instance.get_typed_func(&mut store, "allocate").context("WASM 缺少 allocate(i32) -> i32")?;
		let release: TypedFunc<(i32, i32), ()> = instance.get_typed_func(&mut store, "release").context("WASM 缺少 release(i32, i32)")?;
		let classify: TypedFunc<(i32, i32), i64> = instance.get_typed_func(&mut store, "classify_deck").context("WASM 缺少 classify_deck(i32, i32) -> i64")?;
		if instance.get_export(&mut store, "_initialize").is_some() {
			let initialize: TypedFunc<(), ()> = instance.get_typed_func(&mut store, "_initialize").context("WASM _initialize 必须为 () -> ()")?;
			initialize.call(&mut store, ()).context("WASM _initialize 执行失败")?;
		}
		let cards: Arc<CardsSnapshot> = srvpro3_cards::get()?;
		Ok(Self { cards, store, memory, allocate, release, classify })
	}

	pub fn classify(&mut self, deck: &DeckData) -> Result<Classification> {
		deck.validate()?;
		let input: Input = Input {
			main: Input::section(&deck.main, &self.cards),
			extra: Input::section(&deck.extra, &self.cards),
			side: Input::section(&deck.side, &self.cards),
		};
		let bytes: Vec<u8> = serde_json::to_vec(&input)?;
		ensure!(bytes.len() <= INPUT_LIMIT, "WASM 卡组输入超过 4 MiB");
		self.store.set_fuel(FUEL)?;
		let pointer: i32 = self.allocate.call(&mut self.store, bytes.len() as i32).context("WASM 分配输入内存失败")?;
		ensure!(pointer != 0, "WASM 分配输入内存失败");
		self.memory.write(&mut self.store, pointer as u32 as usize, &bytes).context("WASM 输入内存越界")?;
		let mut output: Option<(i32, i32)> = None;
		let result: Result<Classification> = (|| {
			let packed: u64 = self.classify.call(&mut self.store, (pointer, bytes.len() as i32)).context("WASM 分类执行失败（可能耗尽燃料或发生 trap）")? as u64;
			let address: usize = (packed as u32) as usize;
			let length: usize = (packed >> 32) as usize;
			ensure!(address > 0 && length > 0 && length <= OUTPUT_LIMIT && address.checked_add(length).is_some_and(|end: usize| end <= self.memory.data_size(&self.store)), "WASM 输出地址或长度无效");
			let input_start: usize = pointer as u32 as usize;
			ensure!(address + length <= input_start || address >= input_start + bytes.len(), "WASM 输出不得与输入缓冲区重叠");
			output = Some((address as i32, length as i32));
			let mut data: Vec<u8> = vec![0; length];
			self.memory.read(&self.store, address, &mut data)?;
			let result: Classification = serde_json::from_slice(&data).context("WASM 分类结果必须是包含 id 和 name 的 JSON 对象")?;
			ensure!(!result.id.trim().is_empty() && result.id.len() <= 128 && !result.name.trim().is_empty() && result.name.chars().count() <= 100, "WASM 标目标识或名称无效");
			Ok(result)
		})();
		self.store.set_fuel(FUEL)?;
		if let Some(output) = output { self.release.call(&mut self.store, output).context("WASM 释放输出失败")?; }
		self.release.call(&mut self.store, (pointer, bytes.len() as i32)).context("WASM 释放输入失败")?;
		result
	}
}

