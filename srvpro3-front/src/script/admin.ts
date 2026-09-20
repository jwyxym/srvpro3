import { reactive } from 'vue';

const admin = reactive({
	name : '',
	pass : '',
	to_query : function () {
		if (!this.name || !this.pass)
			return '';
		return `?user=${this.name}&password=${this.pass}`;
	},
	clear : function () {
		this.name = '';
		this.pass = '';
	}
});

export default admin;