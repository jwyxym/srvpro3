import { useDark, useToggle, type UseDarkReturn } from '@vueuse/core';
const dark : UseDarkReturn = useDark();
export const change = useToggle(dark);