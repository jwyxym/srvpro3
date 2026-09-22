<template>
	<div class = 'config_page'>
		<el-alert title = '仅 sudo 管理员可读取、保存及重载配置。账号配置已隐藏并由服务器保留，http_api.webui 和 http_api.port 不允许修改。保存后重载生效。' type = 'info' :closable = 'false'/>
		<el-alert v-if = 'page.error' :title = 'page.error' type = 'error' :closable = 'false'/>
		<div class = 'toolbar'>
			<el-button :disabled = 'page.busy' @click = 'page.load()'>重新读取</el-button>
			<el-button type = 'primary' :disabled = '!page.revision || page.busy || !page.dirty' :loading = 'page.action === "save"' @click = 'page.save()'>保存配置</el-button>
			<el-button type = 'warning' :disabled = '!page.revision || page.busy || page.dirty' :loading = 'page.action === "reload"' @click = 'page.reload()'>重载服务器</el-button>
			<el-text v-if = 'page.dirty' type = 'warning'>有未保存的修改</el-text>
		</div>
		<config-field v-if = 'page.revision' v-model = 'page.values' path = '' :disabled = 'page.busy'/>
	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import { onBeforeRouteLeave } from 'vue-router';
	import { ElMessage, ElMessageBox } from 'element-plus';
	import admin from '@/script/admin';
	import ConfigField from '@/components/config-field.vue';
	import type { ConfigValue } from '@/components/config-field.vue';
	interface ConfigResponse { values : ConfigValue; revision : string }
	const controller = new AbortController();
	const confirm_discard = () => ElMessageBox.confirm('放弃尚未保存的配置修改？', '未保存的修改', {
		confirmButtonText : '放弃修改', cancelButtonText : '继续编辑', type : 'warning'
	}).then(() => true, () => false);
	async function request(path : string, method : string, body? : object) {
		const query = await admin.to_query(path, method, undefined, controller.signal);
		const response = await fetch(path + query, {
			method, signal : controller.signal,
			headers : body ? { 'Content-Type' : 'application/json' } : undefined,
			body : body ? JSON.stringify(body) : undefined,
		});
		if (!response.ok) throw new Error(await response.text() || `请求失败：${response.status}`);
		return response;
	}
	const page = reactive({
		values : {} as ConfigValue, original : '{}', revision : '', error : '', action : '',
		get dirty() : boolean { return page.revision !== '' && JSON.stringify(page.values) !== page.original; },
		get busy() : boolean { return page.action !== ''; },
		load : async () => {
			if (page.busy) return;
			if (page.dirty && !await confirm_discard()) return;
			page.action = 'load';
			page.error = '';
			try {
				const result = await (await request('/config', 'GET')).json() as ConfigResponse;
				page.values = result.values;
				page.original = JSON.stringify(result.values);
				page.revision = result.revision;
			} catch (error) { page.failed(error); }
			finally { page.action = ''; }
		},
		save : async () => {
			if (page.busy || !page.revision) return;
			page.action = 'save';
			page.error = '';
			try {
				const result = await (await request('/config', 'PUT', { values : page.values, revision : page.revision })).json() as ConfigResponse;
				page.values = result.values;
				page.original = JSON.stringify(result.values);
				page.revision = result.revision;
				ElMessage.success('配置已保存，点击重载服务器使其生效');
			} catch (error) { page.failed(error); }
			finally { page.action = ''; }
		},
		reload : async () => {
			if (page.busy || page.dirty || !page.revision) return;
			page.action = 'reload';
			page.error = '';
			try {
				await request('/reload', 'POST');
				ElMessage.success('服务器已重载');
			} catch (error) { page.failed(error); }
			finally { page.action = ''; }
		},
		failed : (error : unknown) => {
			if (!controller.signal.aborted) page.error = error instanceof Error ? error.message : '操作失败';
		},
	});
	const before_unload = (event : BeforeUnloadEvent) => {
		if (page.dirty) { event.preventDefault(); event.returnValue = ''; }
	};
	onBeforeRouteLeave(() => page.dirty ? confirm_discard() : true);
	onMounted(() => { window.addEventListener('beforeunload', before_unload); void page.load(); });
	onUnmounted(() => {
		controller.abort();
		window.removeEventListener('beforeunload', before_unload);
	});
</script>
<style scoped>
	.config_page { display: flex; flex-direction: column; gap: 16px; }
	.toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
	:deep(textarea) { font-family: monospace; tab-size: 4; }
</style>
