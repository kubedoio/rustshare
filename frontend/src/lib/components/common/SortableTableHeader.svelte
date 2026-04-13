<script lang="ts">
	import { ArrowUp, ArrowDown, ArrowUpDown } from 'lucide-svelte';
	import type { SortField, SortOrder } from '$lib/stores/fileSort';

	interface Props {
		label: string;
		field: SortField;
		activeField: SortField;
		activeOrder: SortOrder;
		onSort: (field: SortField) => void;
		class?: string;
	}

	let { label, field, activeField, activeOrder, onSort, class: className = '' }: Props = $props();

	let isActive = $derived(field === activeField);
	let ariaSort: 'ascending' | 'descending' | 'none' = $derived(isActive ? (activeOrder === 'asc' ? 'ascending' : 'descending') : 'none');

	function handleClick() {
		onSort(field);
	}
</script>

<th
	class="px-4 py-2 text-left text-meta font-semibold text-base-content/60 uppercase tracking-wider font-data select-none {className}"
	aria-sort={ariaSort}
>
	<button
		type="button"
		class="flex items-center gap-1 w-full text-left hover:text-base-content transition-colors cursor-pointer"
		onclick={handleClick}
	>
		<span>{label}</span>
		{#if isActive}
			{#if activeOrder === 'asc'}
				<ArrowUp size={12} class="text-brand-500" />
			{:else}
				<ArrowDown size={12} class="text-brand-500" />
			{/if}
		{:else}
			<ArrowUpDown size={12} class="text-base-content/30" />
		{/if}
	</button>
</th>
