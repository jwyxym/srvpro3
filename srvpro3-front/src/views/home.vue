<template>
	<div id = 'srvpro3__home'>
		<aside id = 'srvpro3__home__sidebar' aria-label = '主导航'>
			<div id = 'srvpro3__home__brand'>SrvPro3 <span>WebUI</span></div>
			<el-menu :default-active = 'page.active' @select = 'page.select'>
				<el-menu-item v-for = 'item in page.items' :key = 'item.key' :index = 'item.key'>
					{{ item.label }}
				</el-menu-item>
			</el-menu>
		</aside>
		<div id = 'srvpro3__home__workspace'>
			<header id = 'srvpro3__home__header'>
				<el-button id = 'srvpro3__home__menu' aria-label = '打开导航菜单' :aria-expanded = 'page.drawer_open' @click = 'page.drawer_open = true'>
					<span aria-hidden = 'true'>☰</span>
				</el-button>
				<h1>{{ page.title }}</h1>
			</header>
			<main id = 'srvpro3__home__content'>
				<RouterView/>
			</main>
		</div>
		<el-drawer id = 'srvpro3__home__drawer' v-model = 'page.drawer_open' title = 'SrvPro3 WebUI' direction = 'ltr' size = 'min(280px, 85vw)'>
			<el-menu :default-active = 'page.active' @select = 'page.select'>
				<el-menu-item v-for = 'item in page.items' :key = 'item.key' :index = 'item.key'>
					{{ item.label }}
				</el-menu-item>
			</el-menu>
		</el-drawer>
	</div>
</template>
<script setup lang = 'ts'>
	import { onMounted, onUnmounted, reactive } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import * as ws from '@/script/ws';
	const route = useRoute();
	const router = useRouter();
	const mobile = window.matchMedia('(max-width: 767px)');
	const page = reactive({
		items : [
			{ key : '/home', label : '主页' },
			{ key : '/home/room', label : '房间列表' },
			{ key : '/hoom/history', label : '历史记录' },
		],
		drawer_open : false,
		get active() : string {
			return route.path;
		},
		get title() : string {
			return page.items.find(item => item.key === page.active)?.label ?? '主页';
		},
		select : (key : string) => {
			void router.push(key);
			page.drawer_open = false;
		},
		change : () => {
			if (!mobile.matches) page.drawer_open = false;
		},
	});
	onMounted(() => {
		mobile.addEventListener('change', page.change);
		ws.connect();
	});
	onUnmounted(() => {
		mobile.removeEventListener('change', page.change);
		ws.close();
	});
</script>
<style scoped lang = 'scss'>
	#srvpro3__home {
		display: flex;
		width: 100%;
		height: 100%;
		min-height: 0;
		color: var(--el-text-color-primary);
		#srvpro3__home__sidebar {
			flex: 0 0 220px;
			align-self: stretch;
			height: 100%;
			box-sizing: border-box;
			overflow-y: auto;
			background: var(--el-bg-color);
			border-right: 1px solid var(--el-border-color-light);
			#srvpro3__home__brand {
				padding: 24px 20px;
				font-size: 20px;
				font-weight: 600;
				color: var(--el-color-primary);
				span {
					font-size: 13px;
					color: var(--el-text-color-secondary);
				}
			}
			.el-menu {
				border-right: none;
			}
			@media (max-width: 767px) {
				display: none;
			}
		}
		#srvpro3__home__workspace {
			flex: 1;
			min-width: 0;
			min-height: 0;
			display: flex;
			flex-direction: column;
			#srvpro3__home__header {
				display: flex;
				align-items: center;
				gap: 12px;
				padding: 20px 24px;
				h1 {
					margin: 0;
					font-size: 22px;
					font-weight: 600;
				}
				#srvpro3__home__menu {
					display: none;
					@media (max-width: 767px) {
						display: inline-flex;
					}
				}
				@media (max-width: 767px) {
					padding: 16px;
				}
			}
			#srvpro3__home__content {
				flex: 1;
				min-height: 0;
				overflow: auto;
				padding: 0 24px 24px;
				@media (max-width: 767px) {
					padding: 0 16px 16px;
				}
			}
		}
	}
	#srvpro3__home__drawer {
		.el-menu {
			border-right: none;
		}
	}
</style>
