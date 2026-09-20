import mitt from 'mitt';

export type Events = {
	open : void,
	close : void,
	msg : {
		type : 'room_all' | 'host' | 'room_add' | 'room_close' | 'room_update' | 'history_add' | 'history_update' | 'history_delete' | 'history_reset' | 'cards_upload';
		msg : Object;
	}
};

const emitter = mitt<Events>();
export default emitter;
