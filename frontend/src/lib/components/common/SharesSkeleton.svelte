<div class="mx-auto max-w-6xl space-y-6 p-4 lg:p-6" aria-busy="true" aria-label="Loading shares">
	<!-- Header skeleton -->
	<div class="rounded-[2rem] border border-base-300/70 bg-base-100 p-6 lg:p-8 shadow-panel">
		<div class="flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
			<div class="max-w-2xl space-y-4">
				<div class="h-4 w-40 animate-pulse rounded-full bg-base-200"></div>
				<div class="h-10 w-3/4 animate-pulse rounded bg-base-200"></div>
				<div class="h-4 w-1/2 animate-pulse rounded bg-base-200"></div>
			</div>
			<div class="h-12 w-40 animate-pulse rounded-2xl bg-base-200"></div>
		</div>
	</div>

	<!-- Stats skeleton -->
	<div class="grid gap-4 md:grid-cols-3">
		{#each Array.from({ length: 3 }) as _, i (i)}
			<div class="rounded-[1.5rem] border border-base-300/70 bg-base-100 p-5 shadow-sm">
				<div class="flex items-start justify-between">
					<div class="space-y-3">
						<div class="h-3 w-24 animate-pulse rounded bg-base-200"></div>
						<div class="h-8 w-16 animate-pulse rounded bg-base-200"></div>
					</div>
					<div class="h-12 w-12 animate-pulse rounded-2xl bg-base-200"></div>
				</div>
				<div class="mt-3 h-3 w-48 animate-pulse rounded bg-base-200"></div>
			</div>
		{/each}
	</div>

	<!-- List skeleton -->
	<div class="space-y-4">
		{#each Array.from({ length: 3 }) as _, i (i)}
			<div class="rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm">
				<div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
					<div class="flex items-start gap-4">
						<div class="h-12 w-12 animate-pulse rounded-2xl bg-base-200"></div>
						<div class="space-y-2">
							<div class="h-5 w-48 animate-pulse rounded bg-base-200"></div>
							<div class="h-3 w-32 animate-pulse rounded bg-base-200"></div>
						</div>
					</div>
					<div class="flex gap-2">
						<div class="h-9 w-24 animate-pulse rounded-xl bg-base-200"></div>
						<div class="h-9 w-24 animate-pulse rounded-xl bg-base-200"></div>
					</div>
				</div>
				<div class="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
					{#each Array.from({ length: 4 }) as _, j (j)}
						<div class="h-16 animate-pulse rounded-2xl bg-base-200"></div>
					{/each}
				</div>
			</div>
		{/each}
	</div>
</div>
