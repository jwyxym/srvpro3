import admin from './admin';

const check = new Set<string>();

export const get_host = async () : Promise<Object | void | Error> => {
	let result : true | void | Error = undefined;
	try {
		if (check.has('get_host'))
			return;
		check.add('get_host');
		const i = await fetch('/host' + await admin.to_query('/host'));
		if (!i.ok) {
			const text = await i.text();
			throw new Error(text || `请求失败：${i.status}`)
		}
		result = await i.json();
	} catch (e) {
		result = new Error(e as any);
	} finally {
		check.delete('get_host');
		return result;
	}
};
