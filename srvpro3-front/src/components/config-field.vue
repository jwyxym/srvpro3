<template>
	<section v-if = 'isObject(model)' class = 'group'>
		<h3 v-if = 'path'>{{ title }} <small>{{ path }}</small></h3>
		<template v-for = 'key in fieldKeys' :key = 'key'>
			<config-field :model-value = 'model[key]!' @update:model-value = '(value : ConfigValue) => setField(String(key), value)' :path = 'path ? `${path}.${key}` : String(key)' :disabled = 'disabled'/>
			<el-button v-if = 'path === "cards.excode"' class = 'remove_card' :disabled = 'disabled' type = 'danger' plain @click = 'removeCard(String(key))'>删除卡号 {{ key }}</el-button>
		</template>
		<div v-if = 'path === "cards.excode"' class = 'array_row'>
			<el-input v-model = 'newKey' placeholder = '卡号' :disabled = 'disabled'/>
			<el-button :disabled = 'disabled || !/^\d+$/.test(newKey) || Number(newKey) > 4294967295 || String(Number(newKey)) in model' @click = 'model[String(Number(newKey))] = []; newKey = ""'>添加卡号</el-button>
		</div>
	</section>
	<div v-else class = 'field'>
		<label :for = 'inputId'>{{ title }} <small>{{ path }}</small></label>
		<div v-if = 'Array.isArray(model)' class = 'array'>
			<div v-for = '(_, index) in model' :key = 'index' class = 'array_row'>
				<el-input-number v-if = 'typeof model[index] === "number"' :model-value = 'model[index] as number' :precision = '0' :disabled = 'disabled' @update:model-value = '(value : number | undefined) => setItem(index, value ?? 0)'/>
				<el-input v-else :model-value = 'String(model[index])' :disabled = 'disabled' :aria-label = '`${title} ${index + 1}`' @update:model-value = '(value : string) => setItem(index, value)'/>
				<el-button :disabled = 'disabled' type = 'danger' plain @click = 'model.splice(index, 1)'>删除</el-button>
			</div>
			<el-button :disabled = 'disabled' @click = 'model.push(path.startsWith("cards.excode.") ? 0 : "")'>添加一项</el-button>
		</div>
		<el-switch v-else-if = 'typeof model === "boolean"' :id = 'inputId' v-model = 'model' :disabled = 'disabled || path === "http_api.webui"'/>
		<el-input-number v-else-if = 'typeof model === "number"' :id = 'inputId' :model-value = 'model' :precision = '0' :disabled = 'disabled || path === "http_api.port"' @update:model-value = '(value : number | undefined) => model = value ?? 0'/>
		<el-input v-else :id = 'inputId' :model-value = 'String(model)' :type = 'secret ? "password" : path === "server.welcome" ? "textarea" : "text"' :show-password = 'secret' :autosize = '{ minRows : 2, maxRows : 6 }' :disabled = 'disabled' @update:model-value = '(value : string) => model = value'/>
		<small v-if = 'path === "http_api.webui" || path === "http_api.port"'>此项禁止通过 WebUI 修改</small>
	</div>
</template>
<script setup lang = 'ts'>
	import { computed, ref } from 'vue';
	export type ConfigValue = string | number | boolean | ConfigValue[] | { [key : string] : ConfigValue };
	const model = defineModel<ConfigValue>({ required : true });
	const props = defineProps<{ path : string; disabled : boolean }>();
	const newKey = ref('');
	const isObject = (value : ConfigValue) : value is { [key : string] : ConfigValue } => typeof value === 'object' && !Array.isArray(value);
	const fieldKeys = computed(() => {
		const keys = isObject(model.value) ? Object.keys(model.value) : [];
		if (props.path !== 'http_api') return keys;
		const first = ['webui', 'port'];
		return [...first.filter(key => keys.includes(key)), ...keys.filter(key => !first.includes(key))];
	});
	const labels : Record<string, string> = {
		server : '游戏服务器',
		http_api : 'HTTP 接口',
		db : '数据库',
		redis : 'Redis',
		cards : '卡片资源',
		windbot : '机器人',
		tournament : '比赛',
		'server.tcp' : 'TCP',
		'server.udp' : 'KCP / UDP',
		'server.ws' : 'WebSocket',
		'cards.excode' : '额外系列字段',
		welcome : '欢迎消息',
		tips : '提示文件',
		dialogues : '召唤台词文件',
		watch : '允许中途观战',
		replay : '保存录像',
		bo : 'BO 局数',
		lflist : '禁限卡表索引',
		ot : '卡片范围',
		shuffle : '洗牌',
		master_rule : '大师规则',
		draw_count : '每回合抽卡数',
		start_hand : '起手卡数',
		time_limit : '回合时限（秒）',
		start_lp : '初始生命值',
		reconnect_timeout : '重连保留时间（秒）',
		side_timeout : '换副超时（秒）',
		port : '端口',
		address : '主机地址',
		password : '密码',
		user : '用户名',
		name : '数据库名称 / 文件路径',
		'db.db' : '数据库类型',
		'redis.db' : '数据库编号',
		room : '启用房间接口',
		webui : '启用 WebUI',
		history : '启用历史记录接口',
		enabled : '启用比赛匹配',
		url : '接口地址',
		token : 'API Key',
		tournament_id : '比赛 ID',
		check_deck : '校验登记卡组',
		post_score : '回传比赛成绩',
		timeout : '请求超时（秒）',
		reload : '资源自动重载间隔（秒）',
		expansions : '资源目录',
		ypk : '加载 YPK / ZIP 卡包',
		path : '动态库目录',
		bots : '机器人列表文件',
	};
	const title = computed(() => labels[props.path] ?? labels[props.path.split('.').at(-1)!] ?? props.path.split('.').at(-1)!);
	const inputId = computed(() => `config-${props.path}`);
	const secret = computed(() => /\.(password|token)$/.test(props.path));
	const setField = (key : string, value : ConfigValue) => {
		if (isObject(model.value)) model.value[key] = value;
	};
	const removeCard = (key : string) => {
		if (!props.disabled && props.path === 'cards.excode' && isObject(model.value)) delete model.value[key];
	};
	const setItem = (index : number, value : ConfigValue) => {
		if (Array.isArray(model.value)) model.value[index] = value;
	};
</script>
<style scoped lang = 'scss'>
	.group, .field {
		display: flex;
		flex-direction: column;
		small {
			color: var(--el-text-color-secondary);
			font-size: 12px;
			font-weight: normal;
			overflow-wrap: anywhere;
		}
		.array_row {
			display: flex;
			gap: 8px;
			width: 100%;
		}
		.el-input, .el-textarea {
			max-width: 720px;
		}
	}
	.group {
		gap: 18px;
		.group {
			border: 1px solid var(--el-border-color);
			padding: 20px;
			border-radius: 8px;
		}
		h3 {
			margin: 0;
			font-size: 17px;
		}
		.remove_card {
			align-self: flex-start;
			margin-left: 0;
		}
	}
	.field {
		align-items: flex-start;
		gap: 8px;
		label {
			display: flex;
			flex-wrap: wrap;
			gap: 10px;
			font-size: 14px;
		}
		.array {
			display: flex;
			flex-direction: column;
			align-items: flex-start;
			gap: 8px;
			width: 100%;
		}
	}
</style>
