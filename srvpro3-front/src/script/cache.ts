import { shallowReactive } from 'vue';

const cache = shallowReactive(new Map<any, any>());
export default cache;
