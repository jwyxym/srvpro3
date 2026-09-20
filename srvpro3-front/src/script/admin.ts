import { reactive } from 'vue';

const admin = reactive({
	name : '',
	pass : '',
	to_query : function () {
		return `?user=${this.name}&password=${this.pass}`
	}
});

export default admin;