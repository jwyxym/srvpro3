<template>
	<div id = 'srvpro3__cards'>
		<el-card shadow = 'never'>
			<form class = 'search' @submit.prevent = 'page.search'>
				<el-input v-model = 'page.input' placeholder = '输入卡号、卡名或效果文本' aria-label = '卡片关键词' clearable maxlength = '100'/>
				<el-select v-model = 'page.input_kind' aria-label = '卡片种类'>
					<el-option label = '全部种类' value = 'all'/>
					<el-option label = '怪兽' value = 'monster'/>
					<el-option label = '魔法' value = 'spell'/>
					<el-option label = '陷阱' value = 'trap'/>
				</el-select>
				<el-button type = 'primary' native-type = 'submit' :loading = 'page.loading'>搜索</el-button>
			</form>
		</el-card>
		<el-alert v-if = 'page.error' :title = 'page.error' type = 'error' :closable = 'false'/>
		<div class = 'summary'><span>共 {{ page.total }} 张卡片</span><el-text type = 'info'>点击卡名查看详情</el-text></div>
		<el-card shadow = 'never' v-loading = 'page.loading'>
			<el-table :data = 'page.list' row-key = 'code' empty-text = '没有找到匹配的卡片'>
				<el-table-column v-if = 'page.has_images' label = '卡图' width = '84'>
					<template #default = '{ row }'>
						<el-image v-if = 'image_url(row)' class = 'card-thumbnail' :src = 'image_url(row)' :alt = 'row.name' fit = 'contain' lazy loading = 'lazy' :preview-src-list = '[image_url(row)]' preview-teleported referrerpolicy = 'no-referrer'>
							<template #error><span class = 'image-error'>暂无卡图</span></template>
						</el-image>
					</template>
				</el-table-column>
				<el-table-column prop = 'code' label = '卡号' width = '115'/>
				<el-table-column label = '卡名' min-width = '180'>
					<template #default = '{ row }'><el-button link type = 'primary' class = 'card-name' @click = 'page.open(row)'>{{ row.name || '未命名卡片' }}</el-button></template>
				</el-table-column>
				<el-table-column label = '种类' min-width = '160'><template #default = '{ row }'>{{ card_type(row.card_type) }}</template></el-table-column>
				<el-table-column label = '等级 / 阶级 / LINK' width = '160'><template #default = '{ row }'>{{ monster(row) ? row.level : '—' }}</template></el-table-column>
				<el-table-column label = 'ATK / DEF' width = '150'><template #default = '{ row }'>{{ monster(row) ? `${stat(row.attack)} / ${link(row) ? '—' : stat(row.defense)}` : '—' }}</template></el-table-column>
				<el-table-column prop = 'desc' label = '效果文本' min-width = '260' show-overflow-tooltip/>
				<el-table-column prop = 'pack' label = '来源卡包' min-width = '160' show-overflow-tooltip/>
				<el-table-column prop = 'source' label = '来源路径' min-width = '200' show-overflow-tooltip/>
			</el-table>
		</el-card>
		<div class = 'pagination'><el-pagination v-model:current-page = 'page.current' v-model:page-size = 'page.size' :page-sizes = '[20, 60, 100]' :total = 'page.total' layout = 'total, sizes, prev, pager, next' @current-change = 'page.load' @size-change = 'page.resize'/></div>
		<el-drawer v-model = 'page.drawer' title = '卡片详情' size = 'min(560px, 100vw)' append-to-body destroy-on-close @close = 'usage.reset'>
			<article v-if = 'page.selected' class = 'card-detail'>
				<h2>{{ page.selected.name || '未命名卡片' }}</h2>
				<el-image v-if = 'image_url(page.selected)' :key = 'image_url(page.selected)' class = 'card-art' :src = 'image_url(page.selected)' :alt = 'page.selected.name' fit = 'contain' lazy loading = 'lazy' :preview-src-list = '[image_url(page.selected)]' preview-teleported referrerpolicy = 'no-referrer'>
					<template #error><span class = 'image-error'>暂无卡图</span></template>
				</el-image>
				<el-button text @click = 'page.copy'>卡号 {{ page.selected.code }} · 复制</el-button>
				<p>{{ card_type(page.selected.card_type) }}</p>
				<template v-if = 'monster(page.selected)'>
					<p>{{ attribute(page.selected.attribute) }} · {{ link(page.selected) ? 'LINK' : (page.selected.card_type & 0x800000) ? '阶级' : '等级' }} {{ page.selected.level }}</p>
					<p>ATK {{ stat(page.selected.attack) }}<template v-if = '!link(page.selected)'> / DEF {{ stat(page.selected.defense) }}</template></p>
					<p v-if = 'page.selected.card_type & 0x1000000'>灵摆刻度 {{ page.selected.lscale }} / {{ page.selected.rscale }}</p>
				</template>
				<p v-if = 'page.selected.alias'>同名卡号：{{ page.selected.alias }}</p>
				<p v-if = 'page.selected.pack'>来源卡包：{{ page.selected.pack }}</p>
				<p v-if = 'page.selected.source' class = 'source-path'>来源路径：{{ page.selected.source }}</p>
				<el-divider/>
				<p class = 'description'>{{ page.selected.desc || '暂无效果文本' }}</p>
				<el-divider/>
				<section class = 'usage-panel' aria-label = '卡片使用率查询'>
					<h3>使用率查询</h3>
					<el-text type = 'info'>日期留空默认统计服务器当月；范围含开始日期，不含结束日期。</el-text>
					<el-form label-position = 'top' @submit.prevent = 'usage.load'>
						<el-form-item label = '对局类型'>
							<el-select v-model = 'usage.kind' aria-label = '统计对局类型' @change = 'usage.reset'>
								<el-option label = '全部类型' value = ''/>
								<el-option label = 'Single · 单局' value = 'Single'/>
								<el-option label = 'Match · 多局比赛' value = 'Match'/>
								<el-option label = 'Tag · 双打' value = 'Tag'/>
							</el-select>
						</el-form-item>
						<div class = 'usage-dates'>
							<el-form-item label = '开始日期（包含）'><el-date-picker v-model = 'usage.start' type = 'date' value-format = 'YYYY-MM-DD' placeholder = '默认当月 1 日' aria-label = '统计开始日期' @change = 'usage.reset'/></el-form-item>
							<el-form-item label = '结束日期（不包含）'><el-date-picker v-model = 'usage.end' type = 'date' value-format = 'YYYY-MM-DD' placeholder = '默认下月 1 日' aria-label = '统计结束日期' @change = 'usage.reset'/></el-form-item>
						</div>
						<el-button type = 'primary' native-type = 'submit' :loading = 'usage.loading'>查询使用率</el-button>
					</el-form>
					<el-alert v-if = 'usage.error' :title = 'usage.error' type = 'error' :closable = 'false'/>
					<template v-if = 'usage.result'>
						<el-statistic title = '卡组使用率' :value = 'usage.result.usage_rate' :precision = '2' suffix = '%'/>
						<PieChart title = '使用 / 未使用该卡' :data = 'usage.result.usage_distribution ?? []'/>
						<PieChart title = '使用该卡的卡组标目分布' :data = 'usage.result.archetypes ?? []'/>
						<el-alert v-if = 'usage.result.classification_enabled === false' title = '服务端未启用 WASM 分类器，卡组标目显示为未分类。' type = 'info' :closable = 'false'/>
						<el-descriptions :column = '1' border>
							<el-descriptions-item label = '统计卡号'>{{ usage.result.id }}</el-descriptions-item>
							<el-descriptions-item label = '对局类型'>{{ usage.result.room_type || '全部类型' }}</el-descriptions-item>
							<el-descriptions-item label = '统计范围'>{{ usage.period }}</el-descriptions-item>
							<el-descriptions-item label = '历史小局数'>{{ usage.result.total_duels }}</el-descriptions-item>
							<el-descriptions-item label = '有效卡组数'>{{ usage.result.total_decks }}</el-descriptions-item>
							<el-descriptions-item label = '使用该卡的卡组数'>{{ usage.result.used_decks }}</el-descriptions-item>
							<el-descriptions-item label = '投入总张数'>{{ usage.result.copies }}</el-descriptions-item>
						</el-descriptions>
						<el-alert v-if = 'usage.result.total_decks === 0' title = '当前条件下没有有效卡组样本' type = 'info' :closable = 'false'/>
						<el-text v-if = 'usage.result.skipped_decks' type = 'warning'>已排除 {{ usage.result.skipped_decks }} 个空卡组或损坏卡组。</el-text>
						<el-text v-if = 'usage.result.unknown_type_duels' type = 'warning'>时间范围内有 {{ usage.result.unknown_type_duels }} 条历史小局无法识别类型，指定类型查询时不计入统计。</el-text>
						<el-text type = 'info'>使用率 = 使用该卡的卡组数 ÷ 有效卡组数。主卡组、额外和副卡组均计入；同一卡组投入多张只计一次。每个历史小局独立统计。</el-text>
					</template>
				</section>
			</article>
		</el-drawer>
	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import { ElMessage } from 'element-plus';
	import admin from '@/script/admin';
	import PieChart from '@/components/pie-chart.vue';
	import type { Statistics } from '@/script/deck-stats';
	type Kind = 'all' | 'monster' | 'spell' | 'trap';
	type RoomType = 'Single' | 'Match' | 'Tag';
	interface UsageResult extends Statistics { id : number; }
	interface UsagePage {
		kind : RoomType | ''; start : string | null; end : string | null;
		loading : boolean; error : string; result : UsageResult | null; period : string;
		reset : () => void; load : () => Promise<void>;
	}
	interface Card {
		pack ?: string; source ?: string; image_url ?: string;
		code : number; name : string; desc : string; alias : number; card_type : number;
		level : number; attribute : number; race : number; attack : number; defense : number;
		lscale : number; rscale : number; link_marker : number;
	}
	interface SearchResult { list : Card[]; total : number; image_url ?: string; }
	interface SearchPage {
		input : string; input_kind : Kind; query : string; kind : Kind;
		list : Card[]; total : number; current : number; size : number;
		image_url : string;
		has_images : boolean;
		loading : boolean; error : string; drawer : boolean; selected : Card | null;
		search : () => Promise<void>; load : () => Promise<void>; resize : () => Promise<void>;
		open : (card : Card) => void; copy : () => Promise<void>;
	}
	const monster = (card : Card) : boolean => (card.card_type & 1) !== 0;
	const link = (card : Card) : boolean => (card.card_type & 0x4000000) !== 0;
	const stat = (value : number) : string => value < 0 ? '?' : String(value);
	const type_labels : [number, string][] = [
		[1, '怪兽'], [2, '魔法'], [4, '陷阱'], [0x10, '通常'], [0x20, '效果'],
		[0x40, '融合'], [0x80, '仪式'], [0x200, '灵魂'], [0x400, '同盟'], [0x800, '二重'],
		[0x1000, '调整'], [0x2000, '同调'], [0x4000, '衍生物'], [0x10000, '速攻'],
		[0x20000, '永续'], [0x40000, '装备'], [0x80000, '场地'], [0x100000, '反击'],
		[0x200000, '反转'], [0x400000, '卡通'], [0x800000, '超量'], [0x1000000, '灵摆'], [0x4000000, '连接'],
	];
	const card_type = (value : number) : string => type_labels.filter(([flag] : [number, string]) : boolean => (value & flag) !== 0).map((entry : [number, string]) : string => entry[1]).join(' / ') || '未知';
	const attributes : Record<number, string> = { 1 : '地', 2 : '水', 4 : '炎', 8 : '风', 16 : '光', 32 : '暗', 64 : '神' };
	const attribute = (value : number) : string => attributes[value] ?? '未知属性';
	const image_url = (card : Card) : string => {
		const template : string = card.image_url ?? page.image_url;
		if (!template) return '';
		const address : string = template.includes('{code}') ? template.replaceAll('{code}', String(card.code)) : `${template.replace(/\/+$/, '')}/${card.code}.jpg`;
		try {
			const url : URL = new URL(address, window.location.origin);
			return ['http:', 'https:'].includes(url.protocol) ? url.href : '';
		} catch { return ''; }
	};
	let request : AbortController | null = null;
	const page : SearchPage = reactive<SearchPage>({
		get has_images() : boolean { return page.list.some((card : Card) : boolean => image_url(card) !== ''); },
		input : '', input_kind : 'all', query : '', kind : 'all', list : [], total : 0,
		current : 1, size : 20, loading : false, error : '', drawer : false, selected : null, image_url : '',
		search : async function () : Promise<void> {
			page.query = page.input.trim();
			page.kind = page.input_kind;
			page.current = 1;
			await page.load();
		},
		resize : async function () : Promise<void> {
			page.current = 1;
			await page.load();
		},
		load : async function () : Promise<void> {
			request?.abort();
			const controller : AbortController = new AbortController();
			request = controller;
			page.loading = true;
			page.error = '';
			try {
				const path : string = '/cards/search';
				const params : URLSearchParams = new URLSearchParams({ q : page.query, kind : page.kind, page : String(page.current - 1), page_size : String(page.size) });
				const query : string = await admin.to_query(path, 'GET', params, controller.signal);
				const response : Response = await fetch(path + query, { signal : controller.signal });
				if (!response.ok) throw new Error(await response.text() || `搜索失败：${response.status}`);
				const result : SearchResult = await response.json() as SearchResult;
				if (controller.signal.aborted) return;
				page.list = result.list;
				page.total = result.total;
				page.image_url = result.image_url?.trim() ?? '';
			} catch (error : unknown) {
				if (!controller.signal.aborted) {
					page.error = error instanceof Error ? error.message : '搜索卡片失败';
					page.list = [];
					page.total = 0;
				}
			} finally { if (request === controller) page.loading = false; }
		},
		open : function (card : Card) : void { usage.reset(); page.selected = card; page.drawer = true; },
		copy : async function () : Promise<void> {
			if (!page.selected) return;
			try { await navigator.clipboard.writeText(String(page.selected.code)); ElMessage.success('卡号已复制'); }
			catch { ElMessage.error('复制失败，请手动复制卡号'); }
		},
	});
	let usage_request : AbortController | null = null;
	const usage : UsagePage = reactive<UsagePage>({
		kind : '', start : null, end : null, loading : false, error : '', result : null,
		get period() : string {
			if (!usage.result) return '';
			return `${new Date(usage.result.start * 1000).toLocaleString()} ≤ 时间 < ${new Date(usage.result.end * 1000).toLocaleString()}（本地显示）`;
		},
		reset : function () : void {
			usage_request?.abort();
			usage_request = null;
			usage.loading = false;
			usage.error = '';
			usage.result = null;
		},
		load : async function () : Promise<void> {
			usage.reset();
			if (!page.selected) return;
			if (usage.start && usage.end && usage.start >= usage.end) {
				usage.error = '开始日期必须早于结束日期';
				return;
			}
			const controller : AbortController = new AbortController();
			usage_request = controller;
			usage.loading = true;
			try {
				const path : string = '/cards/usage';
				const params : URLSearchParams = new URLSearchParams({ id : String(page.selected.code) });
				if (usage.kind) params.set('room_type', usage.kind);
				if (usage.start) params.set('start', usage.start);
				if (usage.end) params.set('end', usage.end);
				const query : string = await admin.to_query(path, 'GET', params, controller.signal);
				const response : Response = await fetch(path + query, { signal : controller.signal });
				if (!response.ok) throw new Error(await response.text() || `查询失败：${response.status}`);
				const result : UsageResult = await response.json() as UsageResult;
				if (!controller.signal.aborted) usage.result = result;
			} catch (error : unknown) {
				if (!controller.signal.aborted) usage.error = error instanceof Error ? error.message : '查询使用率失败';
			} finally { if (usage_request === controller) usage.loading = false; }
		},
	});
	onMounted(() : void => { void page.search(); });
	onUnmounted(() : void => { request?.abort(); usage.reset(); });
</script>
<style scoped lang = 'scss'>
	#srvpro3__cards {
		display: flex;
		flex-direction: column;
		gap: 16px;
		.search { display: flex; gap: 12px; }
		.search .el-input { flex: 1; }
		.search .el-select { width: 140px; }
		.summary { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; }
		.pagination { overflow-x: auto; }
		.card-name { white-space: normal; text-align: left; line-height: 1.5; }
		.card-thumbnail { width: 52px; height: 76px; vertical-align: middle; }
		@media (max-width: 600px) {
			.search { flex-wrap: wrap; }
			.search .el-input { flex-basis: 100%; }
			.search .el-select { flex: 1; }
		}
	}
	.card-detail {
		.usage-panel { display: flex; flex-direction: column; gap: 16px; }
		.usage-panel h3 { margin: 0; }
		.usage-dates { display: flex; gap: 12px; flex-wrap: wrap; }
		.usage-dates .el-form-item { flex: 1; min-width: 180px; }
		.usage-dates :deep(.el-date-editor) { width: 100%; }
		.source-path { overflow-wrap: anywhere; }
		h2 { margin-top: 0; overflow-wrap: anywhere; }
		.card-art { display: block; width: min(100%, 320px); height: 460px; margin: 16px auto; }
		.description { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.8; }
	}
	.image-error { display: flex; align-items: center; justify-content: center; height: 100%; font-size: 12px; color: var(--el-text-color-secondary); background: var(--el-fill-color-light); }
</style>
