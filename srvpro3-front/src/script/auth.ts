const encode = (value : ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(value instanceof Uint8Array ? value : value)))
	.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const decode = (value : string) => Uint8Array.from(atob(value.replace(/-/g, '+').replace(/_/g, '/')), char => char.charCodeAt(0));

// 每次请求（包括 WS 重连）都使用新的挑战值和加密密钥，不缓存可重放的 URL。
export const encrypt_query = async (user : string, password : string, path : string, method = 'GET', query = new URLSearchParams(), signal? : AbortSignal) => {
	if (!crypto.subtle) throw new Error('公钥鉴权需要 HTTPS 或 localhost 安全环境');
	if (!user || !password) throw new Error('请输入账号和密码');
	const response = await fetch('/auth/public-key', { cache : 'no-store', signal });
	if (!response.ok) throw new Error(await response.text() || '获取鉴权公钥失败');
	const { public_key, challenge } = await response.json() as { public_key : string; challenge : string };
	const rsa_key = await crypto.subtle.importKey('spki', decode(public_key), { name : 'RSA-OAEP', hash : 'SHA-256' }, false, ['encrypt']);
	const aes_key = await crypto.subtle.generateKey({ name : 'AES-GCM', length : 256 }, true, ['encrypt']);
	const iv = crypto.getRandomValues(new Uint8Array(12));
	const params = new URLSearchParams(query);
	params.delete('auth');
	const target = path + (params.size ? '?' + params.toString() : '');
	const plaintext = new TextEncoder().encode(JSON.stringify({ user, password, challenge, method : method.toUpperCase(), target }));
	const data = await crypto.subtle.encrypt({ name : 'AES-GCM', iv }, aes_key, plaintext);
	const key = await crypto.subtle.encrypt({ name : 'RSA-OAEP' }, rsa_key, await crypto.subtle.exportKey('raw', aes_key));
	params.set('auth', encode(new TextEncoder().encode(JSON.stringify({ key : encode(key), iv : encode(iv), data : encode(data) }))));
	return '?' + params.toString();
};
