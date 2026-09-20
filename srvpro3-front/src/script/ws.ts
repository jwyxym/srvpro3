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
};

export const connect = () : void => {
	const url = new URL('/ws', window.location.href);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	url.search = new URLSearchParams({ user : admin.name, password : admin.pass }).toString();
	if (ws && connection_url === url.href) return;
	close();
	connection_url = url.href;
	const socket = new ReconnectingWebSocket(url.href);
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
