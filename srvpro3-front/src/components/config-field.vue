<template>
	<section v-if = 'isObject(model)' class = 'group'>
		<h3 v-if = 'path'>{{ title }} <small>{{ path }}</small></h3>
		<small v-if = 'path === "cards.pack_image_urls"'>可填写 expansions 中的资源目录（如 ygopro），或目录加文件名（如 ygopro/custom.ypk）以区分重名卡包。优先级：文件路径 → 资源目录 → 文件名 → 全局图床。留空则跳过此项；支持 {code} 占位符，路径区分大小写。</small>
		<template v-for = 'key in fieldKeys' :key = 'key'>
			<config-field :model-value = 'model[key]!' @update:model-value = '(value : ConfigValue) => setField(String(key), value)' :path = 'path ? `${path}.${key}` : String(key)' :disabled = 'disabled'/>
			<el-button v-if = 'path === "cards.excode"' class = 'remove_card' :disabled = 'disabled' type = 'danger' plain @click = 'removeCard(String(key))'>删除卡号 {{ key }}</el-button>
			<el-button v-if = 'path === "cards.pack_image_urls"' :disabled = 'disabled' type = 'danger' plain @click = 'removePack(String(key))'>删除图床配置 {{ key }}</el-button>
		</template>
		<div v-if = 'path === "cards.excode"' class = 'array_row'>
			<el-input v-model = 'newKey' placeholder = '卡号' :disabled = 'disabled'/>
			<el-button :disabled = 'disabled || !/^\d+$/.test(newKey) || Number(newKey) > 4294967295 || String(Number(newKey)) in model' @click = 'model[String(Number(newKey))] = []; newKey = ""'>添加卡号</el-button>
		</div>
		<div v-if = 'path === "cards.pack_image_urls"' class = 'array_row'>
			<el-input v-model = 'newKey' placeholder = '资源目录或卡包路径，例如 ygopro/custom.ypk' :disabled = 'disabled' aria-label = '资源目录或卡包路径'/>
			<el-button :disabled = 'disabled || !newKey.trim() || newKey.trim() in model' @click = 'model[newKey.trim()] = ""; newKey = ""'>添加卡包图床</el-button>
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
		<small v-if = 'path === "cards.image_url"'>留空不显示卡图。支持地址模板，例如 https://example.com/cards/{code}.jpg；仅填写目录时自动追加卡号.jpg。</small>
		<small v-if = 'path === "cards.classifier_wasm"'>服务器本地 WASM 文件路径，留空禁用分类。不提供内置模块；配置后由后端查询统计时调用 classify_deck 判断卡组标目。</small>
	</div>
</template>
<script setup lang = 'ts'>
	import { computed, ref, type ComputedRef } from 'vue';
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
		replay_storage : '录像存储',
		'replay_storage.mode' : '存储模式（database / local / s3）',
		'replay_storage.local' : '本地文件',
		'replay_storage.local.directory' : '录像目录',
		'replay_storage.s3' : 'S3 对象存储',
		endpoint : 'S3 接口地址',
		'replay_storage.s3.public_url' : '录像公开下载地址（留空使用签名链接）',
		region : 'S3 区域',
		bucket : 'S3 桶名',
		prefix : '对象路径前缀',
		access_key_id : 'Access Key ID',
		secret_access_key : 'Secret Access Key',
		session_token : '临时凭据 Token',
		path_style : '使用路径形式访问桶',
		'replay_storage.s3.presign_download' : '使用临时签名下载链接（关闭使用公开常驻链接）',
		presign_expiry : '下载链接有效期（秒）',
		'server.tcp' : 'TCP',
		'server.udp' : 'KCP / UDP',
		'server.ws' : 'WebSocket',
		'cards.excode' : '额外系列字段',
		'cards.image_url' : '卡片图床地址',
		'cards.pack_image_urls' : '按卡包配置图床',
		'cards.classifier_wasm' : 'WASM 分类器路径（留空禁用）',
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
	const title : ComputedRef<string> = computed(() : string => {
		const key : string = props.path.split('.').at(-1)!;
		return labels[props.path] ?? labels[key] ?? key;
	});
	const inputId = computed(() => `config-${props.path}`);
	const secret : ComputedRef<boolean> = computed(() : boolean => /\.(password|token|access_key_id|secret_access_key|session_token)$/.test(props.path));
	const setField = (key : string, value : ConfigValue) => {
		if (isObject(model.value)) model.value[key] = value;
	};
	const removeCard = (key : string) => {
		if (!props.disabled && props.path === 'cards.excode' && isObject(model.value)) delete model.value[key];
	};
	const removePack = (key : string) : void => {
		if (!props.disabled && props.path === 'cards.pack_image_urls' && isObject(model.value)) delete model.value[key];
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
