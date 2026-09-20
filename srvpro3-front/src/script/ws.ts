import ReconnectingWebSocket from 'reconnecting-websocket';
import admin from './admin';
import emitter from './emit';

let ws : ReconnectingWebSocket | undefined;

export const onmessage : Array<(i : {
	type : 'all' | 'host' | 'add' | 'close' | 'update';
	msg : Object;
}) => void> = [];
export const onopen : Array<() => void> = [];
export const onclose : Array<() => void> = [];

export const connect = () : void => {
	ws = new ReconnectingWebSocket('/ws' + admin.to_query());
	ws.onopen = () => emitter.emit('open');
	ws.onclose = () => emitter.emit('close');
	ws.onmessage = (e) => {
		const i = JSON.parse(e.data);
		emitter.emit('msg', i);
	};
};

export const close = () => ws?.close?.();