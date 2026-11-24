import { createRouter, createWebHistory, type Router } from 'vue-router';
import login from './views/login.vue';
import home from './views/home.vue';

export const router : Router = createRouter({
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
				{ path: 'cards', component: () : Promise<typeof import('./views/home/cards.vue')> => import('./views/home/cards.vue') },
				{ path: 'decks', component: () : Promise<typeof import('./views/home/decks.vue')> => import('./views/home/decks.vue') },
				{ path: 'wasm', component: () : Promise<typeof import('./views/home/wasm.vue')> => import('./views/home/wasm.vue') },
				{ path: '/hoom/history', component: () => import('./views/home/history.vue') },
				{ path: '/home/config', component: () => import('./views/home/config.vue') },
			],
		}
	],
});
