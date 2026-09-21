<template>
	<div id = 'srvpro3__history'>
		<div class = 'toolbar'>
			<span>共 {{ page.total }} 条记录</span>
			<el-tag :type = "page.connected ? 'success' : 'warning'">{{ page.connected ? '实时连接' : '连接中 / 重连中' }}</el-tag>
			<el-button type = 'danger' :loading = 'page.deleting === "all"' :disabled = 'page.deleting !== null || !page.total' @click = 'page.delete_history()'>清空历史记录</el-button>
		</div>
		<el-alert v-if = 'page.error' :title = 'page.error' type = 'error' :closable = 'false'/>
		<el-alert v-if = '!page.connected' title = '实时连接暂不可用，正在尝试重连，数据可能不是最新。' type = 'warning' :closable = 'false'/>
		<el-card shadow = 'never' v-loading = 'page.loading'>
			<el-table :data = 'page.list' row-key = 'id' empty-text = '暂无历史记录'>
				<el-table-column type = 'expand'><template #default = '{ row }'>
					<div class = 'detail'>
						<p>卡组 A（Base64）</p><el-input :model-value = 'row.deck_a' type = 'textarea' readonly autosize/>
						<p>卡组 B（Base64）</p><el-input :model-value = 'row.deck_b' type = 'textarea' readonly autosize/>
						<p>录像</p><el-input v-if = 'row.replay' :model-value = 'row.replay' type = 'textarea' readonly autosize/><span v-else>暂无录像</span>
					</div>
				</template></el-table-column>
				<el-table-column prop = 'id' label = 'ID' width = '90'/>
				<el-table-column prop = 'room_id' label = '房间号' min-width = '130'/>
				<el-table-column prop = 'player_a' label = '玩家 A' min-width = '160'>
					<template #default = '{ row }'>
						<el-text type = 'primary' class = 'player_name' role = 'button' tabindex = '0' @click = 'page.open_deck(row.deck_a, row.player_a)' @keydown.enter.prevent = 'page.open_deck(row.deck_a, row.player_a)' @keydown.space.prevent = 'page.open_deck(row.deck_a, row.player_a)'>{{ row.player_a }}</el-text>
					</template>
				</el-table-column>
				<el-table-column prop = 'player_b' label = '玩家 B' min-width = '160'>
					<template #default = '{ row }'>
						<el-text type = 'primary' class = 'player_name' role = 'button' tabindex = '0' @click = 'page.open_deck(row.deck_b, row.player_b)' @keydown.enter.prevent = 'page.open_deck(row.deck_b, row.player_b)' @keydown.space.prevent = 'page.open_deck(row.deck_b, row.player_b)'>{{ row.player_b }}</el-text>
					</template>
				</el-table-column>
				<el-table-column label = '胜利者' min-width = '160'><template #default = '{ row }'>{{ row.winner_id ?? '无胜者' }}</template></el-table-column>
				<el-table-column label = '时间' min-width = '190'><template #default = '{ row }'>{{ page.time(row.created_at) }}</template></el-table-column>
				<el-table-column label = '操作' width = '100' fixed = 'right'><template #default = '{ row }'><el-button type = 'danger' size = 'small' :loading = 'page.deleting === row.id' :disabled = 'page.deleting !== null' @click = 'page.delete_history(row.id)'>删除</el-button></template></el-table-column>
			</el-table>
		</el-card>
		<div class = 'pagination'><el-pagination v-model:current-page = 'page.current_page' v-model:page-size = 'page.page_size' :page-sizes = '[20, 60, 100]' :total = 'page.total' layout = 'total, sizes, prev, pager, next' @current-change = 'page.refresh' @size-change = 'page.resize'/></div>
	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import { ElMessage, ElMessageBox, ElLoading } from 'element-plus';
	import YGOProDeck from 'ygopro-deck-encode';
	import admin from '@/script/admin';
	import { state } from '@/script/ws';
	import emitter, { type Events } from '@/script/emit';
	interface RecordInfo {
		id : number; room_id : string; player_a : string; player_b : string;
		deck_a : string; deck_b : string; winner_id : string | null; replay : string | null; created_at : number;
	}
	const controller = new AbortController();
	let pending = false;
	let revision = 0;
	const page = reactive({
		list : [] as RecordInfo[],
		total : 0,
		current_page : 1,
		page_size : 20,
		get connected() { return state.connected; },
		loading : false,
		error : '',
		deleting : null as number | 'all' | null,
		delete_history : async (id? : number) => {
			if (page.deleting !== null) return;
			page.deleting = id ?? 'all';
			let loading : ReturnType<typeof ElLoading.service> | undefined;
			try {
				const confirmed = await ElMessageBox.confirm(id === undefined ? '确定清空所有历史记录吗？包括其他分页的记录，删除后无法恢复。' : `确定删除历史记录 ${id} 吗？删除后无法恢复。`, id === undefined ? '清空历史记录' : '删除历史记录', {
					type : 'warning', confirmButtonText : '确认删除', cancelButtonText : '取消',
				}).then(() => true, () => false);
				if (!confirmed) return;
				loading = ElLoading.service({ lock : true, text : id === undefined ? '正在清空历史记录' : '正在删除历史记录' });
				const query = new URLSearchParams();
				query.set(id === undefined ? 'all' : 'id', id === undefined ? 'true' : String(id));
				const response = await fetch('/history' + await admin.to_query('/history', 'DELETE', query), { method : 'DELETE' });
				if (!response.ok) throw new Error(await response.text() || `请求失败：${response.status}`);
				const result = await response.json() as { rows_affected : number };
				ElMessage.success(`已删除 ${result.rows_affected} 条历史记录`);
				void page.refresh();
			} catch (error) {
				ElMessage.error(error instanceof Error ? error.message : '删除历史记录失败');
			} finally {
				loading?.close();
				page.deleting = null;
			}
		},
		open_deck : (value : string, name : string) => {
			try {
				const deck = YGOProDeck.fromEncodedString(value);
				if (!deck.main.length && !deck.extra.length && !deck.side.length) throw new Error('卡组为空');
				deck.name = name;
				window.open(deck.toYGOMobileDeckURL(), '_blank', 'noopener,noreferrer');
			} catch {
				ElMessage.error('无法打开卡组，请检查卡组编码是否有效');
			}
		},
		time : (value : number) => new Date(value * 1000).toLocaleString(),
		resize : () => { page.current_page = 1; void page.refresh(); },
		refresh : async () => {
			pending = true;
			revision++;
			if (page.loading || controller.signal.aborted) return;
			page.loading = true;
			try {
				while (pending && !controller.signal.aborted) {
					pending = false;
					const request_revision = revision;
					const query = new URLSearchParams({ page : String(page.current_page - 1), page_size : String(page.page_size) });
					const response = await fetch('/history' + await admin.to_query('/history', 'GET', query, controller.signal), { signal : controller.signal });
					if (!response.ok) throw new Error(await response.text() || `请求失败：${response.status}`);
					const result = await response.json() as { list : RecordInfo[]; total : number };
					if (request_revision !== revision) continue;
					page.total = result.total;
					const last_page = Math.max(1, Math.ceil(result.total / page.page_size));
					if (page.current_page > last_page) {
						page.current_page = last_page;
						pending = true;
						continue;
					}
					page.list = result.list;
					page.error = '';
				}
			} catch (error) {
				if (!controller.signal.aborted) page.error = error instanceof Error ? error.message : String(error);
			} finally { page.loading = false; }
		},
	});
	const on_message = (message : Events['msg']) => {
		if (['history_add', 'history_update', 'history_delete', 'history_reset'].includes(message.type)) void page.refresh();
	};
	onMounted(() => {
		emitter.on('msg', on_message);
		emitter.on('open', page.refresh);
		void page.refresh();
	});
	onUnmounted(() => {
		controller.abort();
		emitter.off('msg', on_message);
		emitter.off('open', page.refresh);
	});
</script>
<style scoped lang = 'scss'>
	#srvpro3__history {
		display: flex;
		flex-direction: column;
		gap: 16px;
		.toolbar {
			display: flex;
			align-items: center;
			flex-wrap: wrap;
			gap: 12px;
		}
		.detail {
			padding: 0 24px 20px;
			p { color: var(--el-text-color-secondary); }
		}
		.pagination { overflow-x: auto; }
		.player_name { cursor: pointer; }
	}
</style>
