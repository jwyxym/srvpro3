use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use srvpro3_cards::CardsSnapshot;
use srvpro3_deck::Deck as DeckData;
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, component::{Component, ComponentExportIndex, Instance, Linker, ResourceTable, TypedFunc}, error::Context as WasmContext};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView, p2};
use super::{Classification, Input, INPUT_LIMIT, OUTPUT_LIMIT};

const FUEL: u64 = 100_000_000;
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

struct Compiled { hash: Vec<u8>, engine: Engine, component: Component }
type ComponentCache = Option<Arc<Compiled>>;
static COMPONENT: LazyLock<Mutex<ComponentCache>> = LazyLock::new(|| Mutex::new(None));

struct State { limits: StoreLimits, wasi: WasiCtx, table: ResourceTable }

impl WasiView for State {
	fn ctx(&mut self) -> WasiCtxView<'_> {
		WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
	}
}

pub(super) struct ComponentDeck {
	cards: Arc<CardsSnapshot>,
	store: Store<State>,
	classify: TypedFunc<(String,), (String,)>,
}

impl ComponentDeck {
	pub(super) fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
		let hash: Vec<u8> = Sha256::digest(&bytes).to_vec();
		let compiled: Arc<Compiled> = {
			let mut cache: MutexGuard<'_, ComponentCache> = COMPONENT.lock().map_err(|_| anyhow::anyhow!("分类 Component 缓存不可用"))?;
			if let Some(compiled) = cache.as_ref().filter(|compiled| compiled.hash == hash) { compiled.clone() }
			else {
				let mut config: Config = Config::new();
				config.consume_fuel(true).wasm_component_model(true).wasm_exceptions(true);
				let engine: Engine = Engine::new(&config)?;
				let component: Component = Component::new(&engine, &bytes).context("编译分类 Component 失败")?;
				let compiled: Arc<Compiled> = Arc::new(Compiled { hash, engine, component });
				*cache = Some(compiled.clone());
				compiled
			}
		};
		let limits: StoreLimits = StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).instances(32).memories(8).tables(32).table_elements(10000).trap_on_grow_failure(true).build();
		let state: State = State { limits, wasi: WasiCtxBuilder::new().build(), table: ResourceTable::new() };
		let mut store: Store<State> = Store::new(&compiled.engine, state);
		store.limiter(|state: &mut State| &mut state.limits);
		store.set_fuel(FUEL)?;
		let mut linker: Linker<State> = Linker::new(&compiled.engine);
		p2::add_to_linker_sync(&mut linker)?;
		let instance: Instance = linker.instantiate(&mut store, &compiled.component).context("初始化分类 Component 失败")?;
		let interface: ComponentExportIndex = instance.get_export_index(&mut store, None, "srvpro3:plugin/deck").context("Component 缺少 srvpro3:plugin/deck 接口")?;
		let export: ComponentExportIndex = instance.get_export_index(&mut store, Some(&interface), "classify-deck").context("Component 缺少 classify-deck 导出")?;
		let classify: TypedFunc<(String,), (String,)> = instance.get_typed_func(&mut store, export).context("Component classify-deck 必须为 (string) -> string")?;
		let cards: Arc<CardsSnapshot> = srvpro3_cards::get()?;
		Ok(Self { cards, store, classify })
	}

	pub(super) fn cards(&self) -> &CardsSnapshot { &self.cards }

	pub(super) fn classify(&mut self, deck: &DeckData) -> Result<Classification> {
		deck.validate()?;
		let input: Input = Input {
			main: Input::section(&deck.main, &self.cards),
			extra: Input::section(&deck.extra, &self.cards),
			side: Input::section(&deck.side, &self.cards),
		};
		let json: String = serde_json::to_string(&input)?;
		ensure!(json.len() <= INPUT_LIMIT, "WASM 卡组输入超过 4 MiB");
		self.store.set_fuel(FUEL)?;
		let (output,): (String,) = self.classify.call(&mut self.store, (json,)).context("Component 分类执行失败")?;
		ensure!(!output.is_empty() && output.len() <= OUTPUT_LIMIT, "Component 输出长度无效");
		let result: Classification = serde_json::from_str(&output).context("Component 分类结果必须是包含 id 和 name 的 JSON 对象")?;
		ensure!(!result.id.trim().is_empty() && result.id.len() <= 128 && !result.name.trim().is_empty() && result.name.chars().count() <= 100, "WASM 标目标识或名称无效");
		Ok(result)
	}
}
