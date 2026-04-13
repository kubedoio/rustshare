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
		{#each Array.from({ length: totalPages }, (_, i) => i + 1) as page}
			<button
				type="button"
				class="btn btn-sm min-w-[2rem] px-2 font-data text-sm transition-colors"
				class:bg-brand-500={page === currentPage}
				class:text-white={page === currentPage}
				class:hover:bg-brand-600={page === currentPage}
				class:btn-ghost={page !== currentPage}
				aria-current={page === currentPage ? 'page' : undefined}
				aria-label="Page {page}"
				onclick={() => handlePageClick(page)}
			>
				{page}
			</button>
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
