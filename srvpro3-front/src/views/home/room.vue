<template>
	<div id = 'srvpro3__rooms'>
		<div class = 'toolbar'>
			<span>共 {{ page.rooms.length }} 个房间</span>
			<el-tag :type = "page.connected ? 'success' : 'warning'">{{ page.connected ? '实时连接' : '连接中 / 重连中' }}</el-tag>
			<el-button type = 'danger' :loading = 'page.interrupting_all' :disabled = '!page.connected || !page.rooms.length || page.interrupting.size > 0' @click = 'page.interrupt_all'>中止所有房间</el-button>
		</div>
		<el-alert v-if = '!page.connected' title = '连接暂不可用，数据可能不是最新；请确认 WS、房间接口已启用且账号具有读权限。' type = 'warning' :closable = 'false'/>
		<el-card shadow = 'never'>
			<el-table :data = 'page.visible' row-key = 'room_id' empty-text = '暂无房间' :default-expand-all = 'false'>
				<el-table-column prop = 'room_id' label = '房间号' width = '180'>
					<template #default = '{ row }'>
						<el-tooltip :content = 'row.room_id' placement = 'top'>
							<el-text type = 'primary' truncated class = 'room_id' role = 'button' tabindex = '0' :aria-label = '`复制房间号 ${row.room_id}`' @click = 'page.copy_room_id(row.room_id)' @keydown.enter.prevent = 'page.copy_room_id(row.room_id)' @keydown.space.prevent = 'page.copy_room_id(row.room_id)'>{{ row.room_id }}</el-text>
						</el-tooltip>
					</template>
				</el-table-column>
				<el-table-column label = '对战方 A' min-width = '170'><template #default = '{ row }'>{{ page.players(row.player_a) }}</template></el-table-column>
				<el-table-column label = '对战方 B' min-width = '170'><template #default = '{ row }'>{{ page.players(row.player_b) }}</template></el-table-column>
				<el-table-column prop = 'spectators' label = '观战人数' width = '100'/>
				<el-table-column prop = 'connections' label = '连接数' width = '90'/>
				<el-table-column label = '聊天记录' min-width = '150'><template #default = '{ row }'><el-button link type = 'primary' @click = 'page.open_chat(row)'>查看聊天（{{ row.chats.length }}）</el-button></template></el-table-column>
				<el-table-column label = '操作' width = '120' fixed = 'right'>
					<template #default = '{ row }'><el-button type = 'danger' size = 'small' :disabled = 'page.interrupting_all' :loading = 'page.interrupting.has(row.room_id)' @click = 'page.interrupt_room(row.room_id)'>中止房间</el-button></template>
				</el-table-column>
			</el-table>
		</el-card>
		<div class = 'pagination'><el-pagination v-model:current-page = 'page.current_page' v-model:page-size = 'page.page_size' :page-sizes = '[20, 60, 100]' :total = 'page.rooms.length' layout = 'total, sizes, prev, pager, next' @size-change = 'page.current_page = 1'/></div>
		<el-drawer v-model = 'page.drawer_open' :title = '`房间 ${page.selected_room_id} · 聊天记录`' direction = 'rtl' size = 'min(480px, 100vw)' append-to-body destroy-on-close>
			<div id = 'srvpro3__room_chat'>
				<el-empty v-if = '!page.selected_room?.chats.length' description = '暂无聊天记录'/>
				<template v-for = '(chat, index) in page.selected_room?.chats' :key = '`${chat.player_id}-${chat.created_at}-${index}`'>
					<QTip v-if = 'page.show_time(index)' is-time>{{ page.time(chat.created_at) }}</QTip>
					<QText :name = 'chat.name' :avatar = 'page.avatar(chat.player_id)'><span class = 'content'>{{ chat.content }}</span></QText>
				</template>
			</div>
		</el-drawer>
	</div>
</template>
<script setup lang = 'ts'>
	import { reactive, watch } from 'vue';
	import { ElMessage, ElMessageBox, ElLoading } from 'element-plus';
	import { QText, QTip } from 'fake-qq-ui';
	import 'fake-qq-ui/styles/fake-qq-ui.css';
	import 'fake-qq-ui/styles/light.scss';
	import 'fake-qq-ui/styles/dark.scss';
	import { state, type Room } from '@/script/ws';
	import admin from '@/script/admin';
	interface Player { id : number; name : string; slot : number }
	const interrupt_request = async (room_id : string) => {
		const query = new URLSearchParams({ user : admin.name, password : admin.pass, room_id });
		const response = await fetch(`/room?${query}`, { method : 'DELETE' });
		if (!response.ok) throw new Error(await response.text() || `请求失败：${response.status}`);
		const result = await response.json();
		if (!result.interrupted) throw new Error('房间未能中止');
	};
	const page = reactive({
		get rooms() { return state.rooms; },
		interrupting : new Set<string>(),
		interrupting_all : false,
		interrupt_all : async () => {
			if (page.interrupting_all || page.interrupting.size || !page.connected) return;
			const room_ids = page.rooms.map(room => room.room_id);
			if (!room_ids.length) return;
			page.interrupting_all = true;
			let loading : ReturnType<typeof ElLoading.service> | undefined;
			try {
				const confirmed = await ElMessageBox.confirm(`确定中止当前全部 ${room_ids.length} 个房间吗？包括其他分页的房间，将断开其中的玩家。操作期间新建的房间不受影响。`, '中止所有房间', {
					type : 'warning', confirmButtonText : '全部中止', cancelButtonText : '取消',
				}).then(() => true, () => false);
				if (!confirmed) return;
				loading = ElLoading.service({ lock : true, text : '正在中止所有房间' });
				let succeeded = 0;
				const errors : string[] = [];
				for (const room_id of room_ids) {
					try { await interrupt_request(room_id); succeeded++; }
					catch (error) { errors.push(`${room_id}：${error instanceof Error ? error.message : '请求失败'}`); }
				}
				if (errors.length) ElMessage.error(`已中止 ${succeeded} 个房间，${errors.length} 个失败；${errors[0]}`);
				else ElMessage.success(`已中止 ${succeeded} 个房间`);
			} finally {
				loading?.close();
				page.interrupting_all = false;
			}
		},
		interrupt_room : async (room_id : string) => {
			if (page.interrupting_all || page.interrupting.has(room_id)) return;
			page.interrupting.add(room_id);
			let loading : ReturnType<typeof ElLoading.service> | undefined;
			try {
				const confirmed = await ElMessageBox.confirm(`确定中止房间「${room_id}」吗？这将结束该房间并断开玩家连接。`, '中止房间', {
					type : 'warning',
					confirmButtonText : '中止房间',
					cancelButtonText : '取消',
				}).then(() => true, () => false);
				if (!confirmed) return;
				loading = ElLoading.service({ lock : true, text : '正在中止房间' });
				await interrupt_request(room_id);
				ElMessage.success('房间已中止');
			} catch (error) {
				ElMessage.error(error instanceof Error ? error.message : '中止房间失败');
			} finally {
				page.interrupting.delete(room_id);
				loading?.close();
			}
		},
		get connected() { return state.connected; },
		current_page : 1,
		page_size : 20,
		drawer_open : false,
		selected_room_id : '',
		get selected_room() : Room | undefined {
			return page.rooms.find(room => room.room_id === page.selected_room_id);
		},
		open_chat : (room : Room) => {
			page.selected_room_id = room.room_id;
			page.drawer_open = true;
		},
		get visible() : Room[] {
			return page.rooms.slice((page.current_page - 1) * page.page_size, page.current_page * page.page_size);
		},
		players : (players : Player[]) => players.map(player => player.name).join(' / ') || '等待加入',
		copy_room_id : async (room_id : string) => {
			try {
				await navigator.clipboard.writeText(room_id);
				ElMessage.success('房间号已复制');
			} catch {
				ElMessage.error('复制失败，请检查剪贴板权限，并使用 HTTPS 或 localhost 访问');
			}
		},
		avatar : (player_id : number) => {
			const id = String(player_id);
			const font_size = Math.min(24, 52 / id.length);
			const hue = (player_id * 137.508) % 360;
			const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><rect width="64" height="64" rx="32" fill="hsl(${hue}, 55%, 45%)"/><text x="32" y="32" dy=".35em" text-anchor="middle" font-family="sans-serif" font-size="${font_size}" fill="white">${id}</text></svg>`;
			return `data:image/svg+xml;base64,${btoa(svg)}`;
		},
		time : (value : number) => new Date(value * 1000).toLocaleString(),
		show_time : (index : number) => {
			const chats = page.selected_room?.chats;
			const current = chats?.[index];
			const previous = chats?.[index - 1];
			return !!current && (!previous || current.created_at - previous.created_at >= 60);
		},
	});
	watch(() => [page.rooms.length, page.selected_room], () => {
		if (page.drawer_open && !page.selected_room) page.drawer_open = false;
		page.current_page = Math.min(page.current_page, Math.max(1, Math.ceil(page.rooms.length / page.page_size)));
	});
</script>
<style scoped lang = 'scss'>
	#srvpro3__rooms {
		display: flex;
		flex-direction: column;
		gap: 16px;
		.toolbar {
			display: flex;
			align-items: center;
			flex-wrap: wrap;
			gap: 12px;
		}
		.pagination { overflow-x: auto; }
		.room_id {
			display: block;
			max-width: 100%;
			cursor: pointer;
		}
	}
	#srvpro3__room_chat {
		min-height: 100%;
		box-sizing: border-box;
		padding: 16px;
		border-radius: 12px;
		background: var(--el-fill-color-light);
		.content {
			white-space: pre-wrap;
			overflow-wrap: anywhere;
		}
	}
</style>
