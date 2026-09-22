import { createRouter, createWebHistory } from 'vue-router';
import login from './views/login.vue';
import home from './views/home.vue';

export const router = createRouter({
	history: createWebHistory(),
	routes: [
		{ path: '/', redirect: '/login' },
		{ path: '/login', component: login },
		{
			path: '/home',
			component: home,
			children: [
				{ path: '', component: () => import('./views/home/index.vue') },
				{ path: 'room', component: () => import('./views/home/room.vue') },
				{ path: '/hoom/history', component: () => import('./views/home/history.vue') },
				{ path: '/home/config', component: () => import('./views/home/config.vue') },
			],
		}
	],
})
