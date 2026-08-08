<div class="flex flex-col gap-6" aria-busy="true" aria-label="Loading module page">
	<!-- Header skeleton -->
	<div
		class="flex flex-col gap-4 rounded-xl border border-base-300 bg-base-100 p-4 sm:flex-row sm:items-center sm:justify-between lg:p-5"
	>
		<div class="flex min-w-0 flex-col gap-2">
			<div class="h-5 w-48 animate-pulse rounded bg-base-200"></div>
			<div class="h-4 w-72 animate-pulse rounded bg-base-200"></div>
		</div>
		<div class="flex gap-2">
			<div class="h-9 w-28 animate-pulse rounded-lg bg-base-200"></div>
			<div class="h-9 w-28 animate-pulse rounded-lg bg-base-200"></div>
		</div>
	</div>

	<!-- Toolbar skeleton -->
	<div
		class="flex flex-col gap-3 rounded-xl border border-base-300 bg-base-100 p-3 lg:flex-row lg:items-center"
	>
		<div class="h-9 flex-1 animate-pulse rounded-lg bg-base-200"></div>
		<div class="flex gap-2">
			<div class="h-9 w-24 animate-pulse rounded-lg bg-base-200"></div>
			<div class="h-9 w-24 animate-pulse rounded-lg bg-base-200"></div>
		</div>
	</div>

	<!-- Content skeleton -->
	<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
		{#each Array.from({ length: 6 }) as _, i (i)}
			<div class="h-36 animate-pulse rounded-xl bg-base-200"></div>
		{/each}
	</div>
</div>
