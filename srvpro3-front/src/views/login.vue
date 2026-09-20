<template>
	<div id = 'srvpro3__login'>
		<el-text type = 'primary'>
			<h1>
				SrvPro3 WebUI
			</h1>
		</el-text>
		<el-input type = 'primary' v-model = 'admin.name' placeholder = '账号'/>
		<el-input type = 'primary' v-model = 'admin.pass' placeholder = '密码' show-password/>
		<div id = 'srvpro3__login__buttons'>
			<el-button type = 'danger' plain @click = 'admin.clear()'>清空</el-button>
			<el-button type = 'primary' plain @click = 'confrim'>确认</el-button>
		</div>
	</div>
</template>
<script setup lang = 'ts'>
import { useRouter } from 'vue-router';
	import { ElMessage, ElLoading } from 'element-plus';
	import cache from '@/script/cache';
	import admin from '@/script/admin';
	import * as api from '@/script/api';
	const router = useRouter();
	const confrim = () => {
		const loading = ElLoading.service({
			lock : true,
			text : '加载中'
		})
		api.get_host().then(i => {
			loading.close();
			if (i instanceof Error)
				ElMessage({
					message: i.message,
					type: 'error',
				});
			else if (i) {
				cache.set('/host', i);
				router.push('/home');
			}
		})
	};
</script>
<style scoped lang = 'scss'>
	#srvpro3__login {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 30px;
		.el-text {
			font-size: 1.5em;
		}
		.el-input {
			width: 240px;
		}
		#srvpro3__login__buttons {
			width: 200px;
			display: flex;
			justify-content: space-between;
		}
	}
</style>
