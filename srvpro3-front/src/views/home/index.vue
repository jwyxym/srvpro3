<template>
	<div id = 'srvpro3__dashboard' v-loading = 'page.loading'>
		<el-alert v-if = 'page.error' :title = 'page.error' type = 'error' :closable = 'false'/>
		<div class = 'system-bar'>
			<span>系统版本：{{ page.host?.system.version ?? '—' }}</span>
		</div>
		<el-card class = 'metrics' shadow = 'never'>
			<section>
				<div class = 'details'>
					<h2>CPU</h2>
					<p>型号</p><p>{{ page.host?.cpu.model ?? '—' }}</p>
					<dl>
						<div><dt>内核数</dt><dd>{{ page.host?.cpu.cores ?? '—' }}</dd></div>
						<div><dt>主频</dt><dd>{{ page.host ? (page.host.cpu.frequency_mhz / 1000).toFixed(2) + ' GHz' : '—' }}</dd></div>
						<div><dt>使用率</dt><dd>{{ page.percent(page.host?.cpu.usage_percent) }}</dd></div>
						<div><dt>SrvPro3 进程</dt><dd>{{ page.percent(page.host?.cpu.srvpro3_usage_percent) }}</dd></div>
					</dl>
				</div>
				<el-progress type = 'circle' :width = '180' :stroke-width = '14' :percentage = 'page.ring(page.host?.cpu.usage_percent)' color = '#409eff'>
					<div class = 'ring-label'><span>CPU 占用</span><strong>{{ page.percent(page.host?.cpu.usage_percent) }}</strong></div>
				</el-progress>
			</section>
			<section>
				<div class = 'details'>
					<h2>内存</h2>
					<p>总量</p><p>{{ page.memory(page.host?.memory.total_bytes) }}</p>
					<dl>
						<div><dt>使用量</dt><dd>{{ page.memory(page.host?.memory.used_bytes) }}</dd></div>
						<div><dt>使用率</dt><dd>{{ page.percent(page.host?.memory.usage_percent) }}</dd></div>
					</dl>
				</div>
				<el-progress type = 'circle' :width = '180' :stroke-width = '14' :percentage = 'page.ring(page.host?.memory.usage_percent)' color = '#79bbff'>
					<div class = 'ring-label'><span>内存占用</span><strong>{{ page.percent(page.host?.memory.usage_percent) }}</strong></div>
				</el-progress>
			</section>
		</el-card>
		<div class = 'counts'>
			<el-card v-for = 'item in page.count_items' :key = 'item.key' shadow = 'never'>
				<strong>{{ page.counts?.[item.key]?.toLocaleString() ?? '—' }}</strong>
				<span>{{ item.label }}</span>
			</el-card>
		</div>
	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import emitter, { type Events } from '@/script/emit';
	import admin from '@/script/admin';
	import cache from '@/script/cache';
	interface HostInfo {
		system : { version : string };
		cpu : { model : string; cores : number; frequency_mhz : number; usage_percent : number; srvpro3_usage_percent : number };
		memory : { total_bytes : number; used_bytes : number; usage_percent : number };
	}
	interface Counts { cards : number; packs : number; lflists : number }
	const controller = new AbortController();
	const request = async <T,>(path : string) : Promise<T> => {
		if (cache.has(path)) return cache.get(path) as T;
		const response = await fetch(path + await admin.to_query(path, 'GET', undefined, controller.signal), { signal : controller.signal });
		if (!response.ok) throw new Error(await response.text() || `请求失败：${response.status}`);
		const value = await response.json() as T;
		// 请求期间可能已经收到较新的 WS 快照，不能用旧响应覆盖它。
		if (cache.has(path)) return cache.get(path) as T;
		cache.set(path, value);
		return value;
	};
	const page = reactive({
		host : (cache.get('/host') ?? null) as HostInfo | null,
		get counts() : Counts | null { return cache.get('/cards') ?? null; },
		loading : false,
		error : '',
		count_items : [
			{ key : 'cards', label : '已加载卡片' },
			{ key : 'packs', label : '已加载卡包' },
			{ key : 'lflists', label : '已加载禁卡表' },
		] as Array<{ key : keyof Counts; label : string }>,
		percent : (value : number | undefined) => value === undefined ? '—' : `${value.toFixed(2)}%`,
		memory : (value : number | undefined) => value === undefined ? '—' : `${(value / 1024 / 1024).toFixed(2)} MB`,
		ring : (value : number | undefined) => Math.min(100, Math.max(0, value ?? 0)),
		load : async () => {
			if (page.loading) return;
			page.loading = true;
			page.error = '';
			const results = await Promise.allSettled([
				request<HostInfo>('/host').then(value => { page.host = value; }),
				request<Counts>('/cards'),
			]);
			if (!controller.signal.aborted) {
				page.error = results.flatMap(result => result.status === 'rejected' ? [String(result.reason instanceof Error ? result.reason.message : result.reason)] : []).join('；');
			}
			page.loading = false;
		},
	});
	const on_message = (message : Events['msg']) => {
		if (message.type !== 'host' || !page.host) return;
		const data = message.msg as { cpu_usage_percent? : number; srvpro3_cpu_usage_percent? : number; memory_usage_percent? : number };
		if (typeof data.cpu_usage_percent === 'number') page.host.cpu.usage_percent = data.cpu_usage_percent;
		if (typeof data.srvpro3_cpu_usage_percent === 'number') page.host.cpu.srvpro3_usage_percent = data.srvpro3_cpu_usage_percent;
		if (typeof data.memory_usage_percent === 'number') {
			page.host.memory.usage_percent = data.memory_usage_percent;
			page.host.memory.used_bytes = page.host.memory.total_bytes * data.memory_usage_percent / 100;
		}
	};
	onMounted(() => {
		emitter.on('msg', on_message);
		void page.load();
	});
	onUnmounted(() => {
		controller.abort();
		emitter.off('msg', on_message);
	});
</script>
<style scoped lang = 'scss'>
	#srvpro3__dashboard {
		display: flex;
		flex-direction: column;
		gap: 24px;
		.system-bar {
			display: flex;
			align-items: center;
			justify-content: space-between;
			gap: 16px;
			color: var(--el-text-color-secondary);
			span { overflow-wrap: anywhere; }
		}
		.metrics {
			border-radius: 20px;
			section {
				display: flex;
				align-items: center;
				gap: 40px;
				padding: 16px 12px;
				& + section {
					border-top: 1px solid var(--el-border-color-lighter);
					margin-top: 20px;
					padding-top: 28px;
				}
				.details {
					flex: 1;
					min-width: 0;
					h2 { margin: 0 0 24px; font-size: 26px; }
					p { margin: 8px 0; overflow-wrap: anywhere; color: var(--el-text-color-secondary); }
					dl {
						display: grid;
						grid-template-columns: repeat(2, minmax(0, 1fr));
						gap: 28px 24px;
						margin: 28px 0 0;
						div {
							display: flex;
							justify-content: space-between;
							gap: 12px;
							dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; }
						}
					}
				}
				.ring-label {
					display: flex;
					flex-direction: column;
					gap: 12px;
					span { font-size: 13px; color: var(--el-text-color-secondary); }
					strong { font-size: 26px; font-variant-numeric: tabular-nums; }
				}
				@media (max-width: 1100px) {
					.details dl { grid-template-columns: 1fr; }
				}
				@media (max-width: 767px) {
					flex-direction: column;
					gap: 28px;
					padding: 8px 0;
					.details { width: 100%; }
				}
			}
		}
		.counts {
			display: grid;
			grid-template-columns: repeat(3, minmax(0, 1fr));
			gap: 24px;
			.el-card {
				border-radius: 20px;
				text-align: center;
				strong { display: block; margin: 8px 0 24px; font-size: 38px; color: var(--el-color-primary); }
				span { color: var(--el-text-color-secondary); }
			}
			@media (max-width: 767px) {
				grid-template-columns: 1fr;
				gap: 16px;
			}
		}
	}
</style>
