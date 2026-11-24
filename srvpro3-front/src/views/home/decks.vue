<template>
	<div class = 'deck-statistics'>
		<el-card shadow = 'never'>
			<template #header>卡组标目分布</template>
			<el-form label-position = 'top' @submit.prevent = 'page.load'>
				<div class = 'filters'>
					<el-form-item label = '卡号（可选）'><el-input v-model = 'page.id' placeholder = '留空统计全部卡组' clearable inputmode = 'numeric' @input = 'page.reset'/></el-form-item>
					<el-form-item label = '对局类型'><el-select v-model = 'page.kind' @change = 'page.reset'><el-option label = '全部类型' value = ''/><el-option label = 'Single · 单局' value = 'Single'/><el-option label = 'Match · 多局比赛' value = 'Match'/><el-option label = 'Tag · 双打' value = 'Tag'/></el-select></el-form-item>
					<el-form-item label = '开始日期（包含）'><el-date-picker v-model = 'page.start' type = 'date' value-format = 'YYYY-MM-DD' placeholder = '默认当月 1 日' @change = 'page.reset'/></el-form-item>
					<el-form-item label = '结束日期（不包含）'><el-date-picker v-model = 'page.end' type = 'date' value-format = 'YYYY-MM-DD' placeholder = '默认下月 1 日' @change = 'page.reset'/></el-form-item>
				</div>
				<el-button type = 'primary' native-type = 'submit' :loading = 'page.loading'>查询分布</el-button>
			</el-form>
			<p class = 'note'>日期留空使用服务器当月。填写卡号时，仅对使用该卡的卡组统计标目分布；标目由服务器配置的外部 WASM 判断。</p>
		</el-card>
		<el-alert v-if = 'page.error' :title = 'page.error' type = 'error' :closable = 'false'/>
		<template v-if = 'page.result'>
			<el-descriptions :column = '1' border>
				<el-descriptions-item label = '统计范围'>{{ statistics_period(page.result) }}</el-descriptions-item>
				<el-descriptions-item label = '对局类型'>{{ page.result.room_type || '全部类型' }}</el-descriptions-item>
				<el-descriptions-item label = '统计卡号'>{{ page.result.id ?? '全部卡组' }}</el-descriptions-item>
				<el-descriptions-item label = '有效卡组 / 历史小局'>{{ page.result.total_decks }} / {{ page.result.total_duels }}</el-descriptions-item>
			</el-descriptions>
			<div class = 'charts'>
				<PieChart v-if = 'page.result.id !== null' title = '使用 / 未使用指定卡片' :data = 'page.result.usage_distribution'/>
				<PieChart :title = 'page.result.id === null ? "全部卡组标目分布" : "使用指定卡片的卡组标目分布"' :data = 'page.result.archetypes'/>
			</div>
			<el-alert v-if = 'page.result.classification_enabled === false' title = '服务器未启用 WASM 分类器，卡组显示为未分类。可在设置中填写外部 WASM 路径。' type = 'info' :closable = 'false'/>
			<el-text v-if = 'page.result.skipped_decks' type = 'warning'>已排除 {{ page.result.skipped_decks }} 个空卡组或损坏卡组。</el-text>
			<el-text v-if = 'page.result.unknown_type_duels' type = 'warning'>范围内 {{ page.result.unknown_type_duels }} 条历史小局类型未知；指定类型时不计入统计。</el-text>
			<el-text type = 'info'>每个小局的每位玩家卡组计一次，每份卡组按模块返回的唯一标目统计。未启用分类模块时统一显示未分类。</el-text>
		</template>

	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import PieChart from '@/components/pie-chart.vue';
	import { query_stats, statistics_period, type RoomType, type Statistics } from '@/script/deck-stats';
	interface Page {
		id : string; kind : RoomType | ''; start : string | null; end : string | null;
		loading : boolean; error : string; result : Statistics | null;
		reset : () => void; load : () => Promise<void>;
	}
	let request : AbortController | null = null;

	const page : Page = reactive<Page>({
		id : '', kind : '', start : null, end : null, loading : false, error : '', result : null,
		reset : function () : void { request?.abort(); request = null; page.loading = false; page.error = ''; page.result = null; },
		load : async function () : Promise<void> {
			page.reset();
			const id : string = page.id.trim();
			if (id && (!/^\d+$/.test(id) || Number(id) < 1 || Number(id) > 0x0fffffff)) { page.error = '请输入有效卡号'; return; }
			if (page.start && page.end && page.start >= page.end) { page.error = '开始日期必须早于结束日期'; return; }
			const controller : AbortController = new AbortController();
			request = controller;
			page.loading = true;
			try {
				const params : URLSearchParams = new URLSearchParams();
				if (id) params.set('id', id);
				if (page.kind) params.set('room_type', page.kind);
				if (page.start) params.set('start', page.start);
				if (page.end) params.set('end', page.end);
				const result : Statistics = await query_stats('/decks/distribution', params, controller.signal);
				if (!controller.signal.aborted) page.result = result;
			} catch (error : unknown) { if (!controller.signal.aborted) page.error = error instanceof Error ? error.message : '查询分布失败'; }
			finally { if (request === controller) page.loading = false; }
		},
	});
	onMounted(() : void => { void page.load(); });
	onUnmounted(() : void => { page.reset(); });
</script>
<style scoped>
	.deck-statistics { display: flex; flex-direction: column; gap: 16px; }
	.filters { display: flex; gap: 12px; flex-wrap: wrap; }
	.filters .el-form-item { flex: 1; min-width: 180px; }
	.filters :deep(.el-date-editor) { width: 100%; }
	.charts { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 340px), 1fr)); gap: 16px; }
	.note { color: var(--el-text-color-secondary); font-size: 13px; line-height: 1.7; margin-bottom: 0; }
</style>
