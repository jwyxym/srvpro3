import { reactive } from 'vue';
import ReconnectingWebSocket from 'reconnecting-websocket';
import admin from './admin';
import cache from './cache';
import emitter, { type Events } from './emit';

let ws : ReconnectingWebSocket | undefined;

export interface Room {
	room_id : string;
	connections : number;
	player_a : Array<{ id : number; name : string; slot : number }>;
	player_b : Array<{ id : number; name : string; slot : number }>;
	spectators : number;
	chats : Array<{ player_id : number; name : string; content : string; created_at : number }>;
}
export const state = reactive({ connected : false, rooms : [] as Room[] });
let connection_url = '';
let connection_controller : AbortController | undefined;

const wait_retry = (signal : AbortSignal) => new Promise<void>(resolve => {
	if (signal.aborted) { resolve(); return; }
	const finish = () => {
		clearTimeout(timer);
		signal.removeEventListener('abort', finish);
		resolve();
	};
	const timer = setTimeout(finish, 5000);
	signal.addEventListener('abort', finish, { once : true });
});

export const close = () => {
	const socket = ws;
	ws = undefined;
	connection_url = '';
	state.connected = false;
	state.rooms = [];
	if (socket) {
		socket.onopen = socket.onclose = socket.onerror = socket.onmessage = null;
		socket.close();
		emitter.emit('close');
	}
	connection_controller?.abort();
	connection_controller = undefined;
};

export const connect = () : void => {
	const url = new URL('/ws', window.location.href);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	const identity = JSON.stringify([admin.name, admin.pass]);
	if (ws && connection_url === identity) return;
	close();
	connection_url = identity;
	const controller = new AbortController();
	connection_controller = controller;
	const socket = new ReconnectingWebSocket(async () => {
		// URL 获取失败不能抛给重连库，否则其连接锁不会释放。
		while (!controller.signal.aborted) {
			const request_controller = new AbortController();
			const abort = () => request_controller.abort();
			controller.signal.addEventListener('abort', abort, { once : true });
			const timer = setTimeout(abort, 10000);
			try {
				url.search = await admin.to_query('/ws', 'GET', undefined, request_controller.signal);
				return url.href;
			} catch {
				if (controller.signal.aborted) break;
				state.connected = false;
			} finally {
				clearTimeout(timer);
				controller.signal.removeEventListener('abort', abort);
			}
			await wait_retry(controller.signal);
		}
		// 主动关闭时正常结束 URL 获取，重连库会依据 close 状态跳过建连。
		return url.href;
	});
	ws = socket;
	socket.onopen = () => {
		state.connected = true;
		emitter.emit('open');
	};
	socket.onclose = () => {
		state.connected = false;
		emitter.emit('close');
	};
	socket.onerror = () => { state.connected = false; };
	socket.onmessage = (event) => {
		let message : Events['msg'];
		try { message = JSON.parse(event.data); } catch { return; }
		if (!message || typeof message.type !== 'string' || !message.msg || typeof message.msg !== 'object') return;
		// 持续维护房间快照，切换页面无需重连来获取 room_all。
		if (message.type === 'cards_upload') cache.set('/cards', message.msg);
		if (message.type === 'room_all') {
			if (!Array.isArray(message.msg)) return;
			state.rooms = message.msg;
		} else if (['room_add', 'room_update', 'room_close'].includes(message.type)) {
			const room = message.msg as Room;
			if (typeof room.room_id !== 'string') return;
			const index = state.rooms.findIndex(value => value.room_id === room.room_id);
			if (message.type === 'room_close') {
				if (index !== -1) state.rooms.splice(index, 1);
			} else if (index === -1) state.rooms.push(room);
			else state.rooms[index] = room;
		}
		if (message.type.startsWith('room_')) state.rooms.sort((a, b) => a.room_id.localeCompare(b.room_id, undefined, { numeric : true }));
		emitter.emit('msg', message);
	};
};
