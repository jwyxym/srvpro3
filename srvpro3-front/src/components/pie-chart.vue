<template>
	<figure class = 'pie-chart'>
		<figcaption>{{ title }}</figcaption>
		<el-empty v-if = 'total === 0' description = '暂无有效样本' :image-size = '64'/>
		<div v-else class = 'chart-layout'>
			<svg viewBox = '0 0 120 120' role = 'img' :aria-label = '`${title}，共 ${total} 个卡组`'>
				<circle cx = '60' cy = '60' r = '40' fill = 'none' stroke = 'var(--el-fill-color)' stroke-width = '24'/>
				<circle v-for = 'slice in slices' :key = 'slice.id' cx = '60' cy = '60' r = '40' fill = 'none' :stroke = 'slice.color' stroke-width = '24' pathLength = '100' :stroke-dasharray = '`${slice.percent} ${100 - slice.percent}`' :stroke-dashoffset = '-slice.offset' transform = 'rotate(-90 60 60)' tabindex = '0'>
					<title>{{ slice.name }}：{{ slice.value }} 个卡组（{{ slice.percent.toFixed(2) }}%）</title>
				</circle>
				<text x = '60' y = '58' text-anchor = 'middle' class = 'chart-total'>{{ total }}</text>
				<text x = '60' y = '73' text-anchor = 'middle' class = 'chart-label'>卡组样本</text>
			</svg>
			<ul>
				<li v-for = 'slice in slices' :key = 'slice.id'><i :style = '{ backgroundColor : slice.color }'/><span>{{ slice.name }}</span><strong>{{ slice.value }} · {{ slice.percent.toFixed(2) }}%</strong></li>
			</ul>
		</div>
	</figure>
</template>
<script setup lang = 'ts'>
	import { computed, type ComputedRef } from 'vue';
	export interface PieItem { id : string; name : string; value : number; }
	interface Slice extends PieItem { color : string; percent : number; offset : number; }
	const props : { title : string; data : PieItem[] } = defineProps<{ title : string; data : PieItem[] }>();
	const valid : ComputedRef<PieItem[]> = computed(() : PieItem[] => props.data.filter((item : PieItem) : boolean => Number.isFinite(item.value) && item.value > 0));
	const total : ComputedRef<number> = computed(() : number => valid.value.reduce((sum : number, item : PieItem) : number => sum + item.value, 0));
	const slices : ComputedRef<Slice[]> = computed(() : Slice[] => {
		let offset : number = 0;
		return valid.value.map((item : PieItem, index : number) : Slice => {
			const percent : number = item.value / total.value * 100;
			const slice : Slice = { ...item, offset, percent, color : `hsl(${(index * 137.508 + 210) % 360} 62% 48%)` };
			offset += percent;
			return slice;
		});
	});
</script>
<style scoped>
	.pie-chart { margin: 0; padding: 16px; border: 1px solid var(--el-border-color-light); border-radius: 8px; }
	figcaption { font-weight: 600; margin-bottom: 12px; }
	.chart-layout { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }
	svg { width: 200px; max-width: 100%; flex-shrink: 0; margin: auto; }
	circle:focus { outline: none; stroke-width: 28; }
	.chart-total { font-size: 17px; font-weight: 600; fill: var(--el-text-color-primary); }
	.chart-label { font-size: 8px; fill: var(--el-text-color-secondary); }
	ul { list-style: none; padding: 0; margin: 0; flex: 1; min-width: 180px; max-height: 320px; overflow-y: auto; }
	li { display: flex; align-items: center; gap: 8px; margin: 10px 0; font-size: 13px; }
	i { width: 10px; height: 10px; flex-shrink: 0; border-radius: 50%; }
	li span { flex: 1; overflow-wrap: anywhere; }
	strong { font-weight: 500; white-space: nowrap; }
</style>
