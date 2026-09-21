import { reactive } from 'vue';
import { encrypt_query } from './auth';

const admin = reactive({
	name : '',
	pass : '',
	to_query : function (path : string, method = 'GET', query = new URLSearchParams(), signal? : AbortSignal) {
		return encrypt_query(this.name, this.pass, path, method, query, signal);
	},
	clear : function () {
		this.name = '';
		this.pass = '';
	}
});

export default admin;
