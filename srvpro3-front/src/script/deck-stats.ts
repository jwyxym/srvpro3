import admin from './admin';
import type { PieItem } from '@/components/pie-chart.vue';

export type RoomType = 'Single' | 'Match' | 'Tag';
export interface Statistics {
	id : number | null; room_type : RoomType | null; start : number; end : number;
	total_duels : number; total_decks : number; used_decks : number; copies : number;
	skipped_decks : number; unknown_type_duels : number; usage_rate : number;
	usage_distribution : PieItem[]; archetypes : PieItem[]; classification_enabled : boolean;
}

export const query_stats = async (path : string, params : URLSearchParams, signal : AbortSignal) : Promise<Statistics> => {
	const query : string = await admin.to_query(path, 'GET', params, signal);
	const response : Response = await fetch(path + query, { signal });
	if (!response.ok) throw new Error(await response.text() || `统计查询失败：${response.status}`);
	return await response.json() as Statistics;
};

export const statistics_period = (result : Statistics | null) : string => result
	? `${new Date(result.start * 1000).toLocaleString()} ≤ 时间 < ${new Date(result.end * 1000).toLocaleString()}（本地显示）`
	: '';
