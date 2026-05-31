<div
	class="grid grid-cols-1 gap-2 sm:grid-cols-2 sm:gap-3 lg:grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(13rem,1fr))]"
	aria-busy="true"
	aria-label="Loading file grid"
>
	{#each Array.from({ length: 12 }) as _, i (i)}
		<div class="flex flex-col gap-2 rounded-xl border border-base-300/50 bg-base-100 p-3">
			<div class="aspect-square w-full animate-pulse rounded-lg bg-base-200"></div>
			<div class="h-4 w-3/4 animate-pulse rounded bg-base-200"></div>
			<div class="h-3 w-1/2 animate-pulse rounded bg-base-200"></div>
		</div>
	{/each}
</div>
