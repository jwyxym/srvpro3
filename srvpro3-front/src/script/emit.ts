import mitt from 'mitt';

type Events = {
	open : void,
	close : void,
	msg : {
		type : 'all' | 'host' | 'add' | 'close' | 'update';
		msg : Object;
	}
};

const emitter = mitt<Events>();
export default emitter;