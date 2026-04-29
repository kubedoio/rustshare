<div
	class="mx-auto flex max-w-[1200px] flex-col gap-4 px-4 pb-10"
	aria-busy="true"
	aria-label="Loading dashboard"
>
	<!-- Workspace Panel -->
	<div class="flex flex-col gap-5 rounded-3xl border border-base-300 bg-base-100 p-6 shadow-sm">
		<div class="grid gap-8 lg:grid-cols-[1fr_auto]">
			<div class="flex flex-col gap-4">
				<div class="h-6 w-32 animate-pulse rounded-full bg-base-200"></div>
				<div class="h-10 w-72 animate-pulse rounded bg-base-200"></div>
				<div class="h-4 w-96 animate-pulse rounded bg-base-200"></div>
			</div>
			<div class="flex min-w-[240px] flex-col justify-center gap-3">
				<div class="h-24 w-full animate-pulse rounded-xl bg-base-200"></div>
				<div class="h-20 w-full animate-pulse rounded-xl bg-base-200"></div>
			</div>
		</div>
		<div class="grid grid-cols-3 gap-4 border-t border-base-300/50 pt-5">
			<div class="h-24 animate-pulse rounded-xl bg-base-200"></div>
			<div class="h-24 animate-pulse rounded-xl bg-base-200"></div>
			<div class="h-24 animate-pulse rounded-xl bg-base-200"></div>
		</div>
	</div>

	<!-- Notes Panel -->
	<div class="rounded-3xl border border-base-300 bg-base-100 p-6 shadow-sm">
		<div class="mb-4">
			<div class="mb-2 h-5 w-24 animate-pulse rounded bg-base-200"></div>
			<div class="h-3 w-40 animate-pulse rounded bg-base-200"></div>
		</div>
		<div class="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
			{#each Array.from({ length: 8 }) as _, i (i)}
				<div class="h-32 animate-pulse rounded-xl bg-base-200"></div>
			{/each}
		</div>
	</div>
</div>
