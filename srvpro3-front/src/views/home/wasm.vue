<template>
	<div class = 'wasm-test'>
		<el-card shadow = 'never'>
			<template #header>测试卡组分类 WASM</template>
			<el-form label-position = 'top' @submit.prevent = 'page.run()'>
				<p class = 'note'>提交卡组后，由后端运行服务器配置的分类 WASM。请先在服务器设置中填写分类 WASM 路径。</p>
				<div class = 'deck-tools'>
					<el-form-item label = '卡组格式'>
						<el-select v-model = 'page.format' @change = 'page.reset()'><el-option label = 'YDK' value = 'ydk'/><el-option label = 'JSON' value = 'json'/></el-select>
					</el-form-item>
					<el-button @click = 'page.example()'>填入示例卡组</el-button>
					<label class = 'import-deck'>导入卡组文件 <input type = 'file' accept = '.ydk,.json' aria-label = '导入卡组文件' @change = 'page.import_deck($event)'/></label>
				</div>
				<el-form-item label = '测试卡组'>
					<el-input v-model = 'page.text' type = 'textarea' :rows = '12' placeholder = '粘贴 YDK，或包含 main、extra、side 的 JSON 卡组' aria-label = '测试卡组内容' @input = 'page.reset()'/>
				</el-form-item>
				<p class = 'note'>JSON 分区支持卡号数组或带 code 的卡片对象数组。卡片属性统一使用服务器当前卡库；不存在的卡片会标记 missing。示例仅用于验证分类，不检查对局卡组规则。</p>
				<el-button type = 'primary' native-type = 'submit' :loading = 'page.loading'>运行测试</el-button>
			</el-form>
		</el-card>
		<el-card v-if = 'page.error' shadow = 'never' class = 'error' role = 'alert'>
			<template #header>测试失败</template>
			<pre>{{ page.error }}</pre>
		</el-card>
		<el-card v-if = 'page.result' shadow = 'never'>
			<template #header>测试结果</template>
			<el-descriptions :column = '1' border>
				<el-descriptions-item label = '分类名称'>{{ page.result.classification.name }}</el-descriptions-item>
				<el-descriptions-item label = '分类标识'>{{ page.result.classification.id }}</el-descriptions-item>
				<el-descriptions-item label = '模块格式'>{{ page.result.format === 'component' ? 'WASI Component' : '核心 WASM / WASI Preview 1' }}</el-descriptions-item>
				<el-descriptions-item label = '加载及分类耗时'>{{ page.result.elapsed_ms }} ms</el-descriptions-item>
			</el-descriptions>
			<el-collapse class = 'payload'>
				<el-collapse-item title = '查看实际传入模块的卡组 JSON' name = 'input'><pre>{{ JSON.stringify(page.result.input, null, 2) }}</pre></el-collapse-item>
			</el-collapse>
		</el-card>
	</div>
</template>
<script setup lang = 'ts'>
	import { onUnmounted, reactive } from 'vue';
	import admin from '@/script/admin';
	interface DeckInput { main : number[]; extra : number[]; side : number[]; }
	interface TestResult {
		classification : { id : string; name : string };
		format : 'core' | 'component'; elapsed_ms : number;
		input : Record<string, unknown>;
	}
	interface Page {
		format : 'ydk' | 'json';
		text : string; loading : boolean; error : string; result : TestResult | null;
		reset : () => void; example : () => void;
		import_deck : (event : Event) => Promise<void>; run : () => Promise<void>;
	}
	const card_id = (value : unknown) : number => {
		const code : unknown = typeof value === 'object' && value !== null && 'code' in value ? value.code : value;
		if (typeof code !== 'number' || !Number.isInteger(code) || code < 1 || code > 0x0fffffff) throw new Error('卡号须为 1 到 268435455 的整数');
		return code;
	};
	const parse_deck = (text : string, format : 'ydk' | 'json') : DeckInput => {
		let deck : DeckInput = { main : [], extra : [], side : [] };
		if (format === 'json') {
			const value : unknown = JSON.parse(text);
			if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('卡组 JSON 必须是对象');
			const object : Record<string, unknown> = value as Record<string, unknown>;
			for (const section of ['main', 'extra', 'side'] as const) {
				const cards : unknown = object[section] ?? [];
				if (!Array.isArray(cards)) throw new Error(`${section} 必须是数组`);
				deck[section] = (cards as unknown[]).map(card_id);
			}
		} else {
			let section : keyof DeckInput = 'main';
			for (const raw of text.split(/\r?\n/)) {
				const line : string = raw.trim();
				if (line === '#main') section = 'main';
				else if (line === '#extra') section = 'extra';
				else if (line === '!side') section = 'side';
				else if (!line || line.startsWith('#')) continue;
				else if (/^\d+$/.test(line)) deck[section].push(card_id(Number(line)));
				else throw new Error(`无法识别的 YDK 行：${line}`);
			}
		}
		if (!deck.main.length) throw new Error('主卡组不能为空');
		if (deck.main.length + deck.extra.length + deck.side.length > 200) throw new Error('卡组最多允许 200 张卡');
		return deck;
	};
	let request : AbortController | null = null;
	let import_generation : number = 0;
	const page : Page = reactive<Page>({
		format : 'ydk', text : '', loading : false, error : '', result : null,
		reset : function () : void { import_generation++; request?.abort(); request = null; this.loading = false; this.error = ''; this.result = null; },
		example : function () : void { this.reset(); this.format = 'ydk'; this.text = '#main\n89631139\n89631139\n89631139\n#extra\n!side\n'; },
		import_deck : async function (event : Event) : Promise<void> {
			this.reset();
			const generation : number = ++import_generation;
			const input : HTMLInputElement = event.target as HTMLInputElement;
			const file : File | undefined = input.files?.[0];
			input.value = '';
			if (!file) return;
			if (file.size > 1024 * 1024) { this.error = '卡组文件不得超过 1 MiB'; return; }
			try {
				const text : string = await file.text();
				if (generation !== import_generation) return;
				this.format = file.name.toLowerCase().endsWith('.json') ? 'json' : 'ydk';
				this.text = text;
			} catch (error : unknown) { if (generation === import_generation) this.error = error instanceof Error ? error.message : '导入卡组失败'; }
		},
		run : async function () : Promise<void> {
			this.reset();
			const controller : AbortController = new AbortController();
			request = controller;
			this.loading = true;
			try {
				const deck : DeckInput = parse_deck(this.text, this.format);
				const payload : { deck : DeckInput } = { deck };
				if (controller.signal.aborted) return;
				const path : string = '/wasm/test';
				const query : string = await admin.to_query(path, 'POST', new URLSearchParams(), controller.signal);
				const response : Response = await fetch(path + query, { method : 'POST', headers : { 'Content-Type' : 'application/json' }, body : JSON.stringify(payload), signal : controller.signal });
				if (!response.ok) throw new Error(await response.text() || `WASM 测试失败：${response.status}`);
				const result : TestResult = await response.json() as TestResult;
				if (!controller.signal.aborted) this.result = result;
			} catch (error : unknown) { if (!controller.signal.aborted) this.error = error instanceof Error ? error.message : 'WASM 测试失败'; }
			finally { if (request === controller) { this.loading = false; request = null; } }
		},
	});
	onUnmounted(() : void => { import_generation++; page.reset(); });
</script>
<style scoped>
	.wasm-test { display: flex; flex-direction: column; gap: 16px; }
	.note { color: var(--el-text-color-secondary); font-size: 13px; line-height: 1.7; }
	.deck-tools { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; margin: 20px 0; }
	.deck-tools .el-form-item { margin: 0; width: 160px; }
	.import-deck { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: 14px; }
	input[type='file'] { max-width: 100%; }
	.payload { margin-top: 16px; }
	pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 440px; overflow: auto; margin: 0; font-size: 13px; line-height: 1.6; }
	.error { color: var(--el-color-danger); }
</style>
