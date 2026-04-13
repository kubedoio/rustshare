<script lang="ts">
	import { ChevronLeft, ChevronRight } from 'lucide-svelte';

	interface Props {
		currentPage: number;
		totalPages: number;
		pageSize: 10 | 20 | 50;
		onPageChange: (page: number) => void;
		onPageSizeChange: (size: 10 | 20 | 50) => void;
	}

	let { currentPage, totalPages, pageSize, onPageChange, onPageSizeChange }: Props = $props();

	const pageSizeOptions: Array<10 | 20 | 50> = [10, 20, 50];

	function getVisiblePages(currentPage: number, totalPages: number): Array<number | '...'> {
		if (totalPages <= 7) {
			return Array.from({ length: totalPages }, (_, i) => i + 1);
		}

		const pages: Array<number | '...'> = [1];

		if (currentPage <= 3) {
			pages.push(2, 3);
			pages.push('...');
			pages.push(totalPages);
		} else if (currentPage >= totalPages - 2) {
			pages.push('...');
			pages.push(totalPages - 2, totalPages - 1, totalPages);
		} else {
			pages.push('...');
			pages.push(currentPage - 1, currentPage, currentPage + 1);
			pages.push('...');
			pages.push(totalPages);
		}

		return pages;
	}

	function handlePrevious() {
		if (currentPage > 1) {
			onPageChange(currentPage - 1);
		}
	}

	function handleNext() {
		if (currentPage < totalPages) {
			onPageChange(currentPage + 1);
		}
	}

	function handlePageClick(page: number) {
		if (page !== currentPage) {
			onPageChange(page);
		}
	}

	function handlePageSizeChange(e: Event) {
		const target = e.target as HTMLSelectElement;
		const value = Number(target.value) as 10 | 20 | 50;
		onPageSizeChange(value);
	}
</script>

<div class="flex items-center gap-3 h-10" data-testid="pagination-controls">
	<button
		type="button"
		class="btn btn-sm btn-ghost flex items-center gap-1 px-2"
		disabled={currentPage <= 1}
		aria-label="Previous page"
		onclick={handlePrevious}
	>
		<ChevronLeft class="w-4 h-4" />
		<span class="text-meta hidden sm:inline">Previous</span>
	</button>

	<div class="flex items-center gap-1">
		{#each getVisiblePages(currentPage, totalPages) as item}
			{#if item === '...'}
				<span
					class="min-w-[2rem] px-2 font-data text-sm text-ink-muted flex items-center justify-center select-none"
					aria-hidden="true"
				>
					...
				</span>
			{:else}
				<button
					type="button"
					class="btn btn-sm min-w-[2rem] px-2 font-data text-sm transition-colors"
					class:bg-brand-500={item === currentPage}
					class:text-white={item === currentPage}
					class:hover:bg-brand-600={item === currentPage}
					class:btn-ghost={item !== currentPage}
					aria-current={item === currentPage ? 'page' : undefined}
					aria-label="Page {item}"
					onclick={() => handlePageClick(item)}
				>
					{item}
				</button>
			{/if}
		{/each}
	</div>

	<button
		type="button"
		class="btn btn-sm btn-ghost flex items-center gap-1 px-2"
		disabled={currentPage >= totalPages}
		aria-label="Next page"
		onclick={handleNext}
	>
		<span class="text-meta hidden sm:inline">Next</span>
		<ChevronRight class="w-4 h-4" />
	</button>

	<div class="flex items-center gap-2 ml-2">
		<label for="page-size" class="text-meta text-ink-muted">Items per page</label>
		<select
			id="page-size"
			class="select select-sm select-bordered font-data text-sm"
			value={pageSize}
			onchange={handlePageSizeChange}
		>
			{#each pageSizeOptions as size}
				<option value={size}>{size}</option>
			{/each}
		</select>
	</div>
</div>
