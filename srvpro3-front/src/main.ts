import { createApp } from 'vue';
import App from './App.vue';
import ElementPlus from 'element-plus';
import 'element-plus/dist/index.css';
import 'element-plus/theme-chalk/dark/css-vars.css';
import 'element-plus/theme-chalk/base.css';
import { router } from './router';

createApp(App)
	.use(ElementPlus)
	.use(router)
	.mount('#app');
