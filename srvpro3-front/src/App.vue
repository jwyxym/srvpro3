<template>
	<el-config-provider :message = '{ plain : true, duration : 3000, showClose : true, grouping : true, offset : 20 }'>
		<RouterView/>
	</el-config-provider>
</template>
<script setup lang = 'ts'>
	import { onUnmounted } from 'vue';
	import * as ws from './script/ws';
	import './script/height';
	onUnmounted(() => {
		ws.close();
	})
</script>
<style lang = 'scss'>
	body {
		margin: 0;
		width: var(--srvpro3__width);
		height: var(--srvpro3__height);
		overflow: hidden;
		background: linear-gradient(135deg, #f5faff 0%, #ecf5ff 50%, #d9ecff 100%);
		background-attachment: fixed;
		#app {
			width: 100%;
			height: 100%;
		}
		.el-message {
			max-width: calc(100vw - 32px);
			box-sizing: border-box;
			border-radius: 10px;
			.el-message__content {
				min-width: 0;
				line-height: 1.5;
				overflow-wrap: anywhere;
			}
		}
	}
	html.dark body {
		background: linear-gradient(135deg, #141820 0%, #18222f 50%, #1c3045 100%);
	}
</style>
