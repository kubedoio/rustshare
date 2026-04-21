<div class="max-w-[1200px] mx-auto px-4 pb-10 flex flex-col gap-4" aria-busy="true" aria-label="Loading dashboard">
	<!-- Workspace Panel -->
	<div class="bg-base-100 border border-base-300 rounded-3xl p-6 shadow-sm flex flex-col gap-5">
		<div class="grid lg:grid-cols-[1fr_auto] gap-8">
			<div class="flex flex-col gap-4">
				<div class="h-6 w-32 rounded-full bg-base-200 animate-pulse"></div>
				<div class="h-10 w-72 rounded bg-base-200 animate-pulse"></div>
				<div class="h-4 w-96 rounded bg-base-200 animate-pulse"></div>
			</div>
			<div class="flex flex-col gap-3 min-w-[240px] justify-center">
				<div class="h-24 w-full rounded-xl bg-base-200 animate-pulse"></div>
				<div class="h-20 w-full rounded-xl bg-base-200 animate-pulse"></div>
			</div>
		</div>
		<div class="grid grid-cols-3 gap-4 pt-5 border-t border-base-300/50">
			<div class="h-24 rounded-xl bg-base-200 animate-pulse"></div>
			<div class="h-24 rounded-xl bg-base-200 animate-pulse"></div>
			<div class="h-24 rounded-xl bg-base-200 animate-pulse"></div>
		</div>
	</div>

	<!-- Notes Panel -->
	<div class="bg-base-100 border border-base-300 rounded-3xl p-6 shadow-sm">
		<div class="mb-4">
			<div class="h-5 w-24 rounded bg-base-200 animate-pulse mb-2"></div>
			<div class="h-3 w-40 rounded bg-base-200 animate-pulse"></div>
		</div>
		<div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4">
			{#each Array.from({ length: 8 }) as _, i (i)}
				<div class="h-32 rounded-xl bg-base-200 animate-pulse"></div>
			{/each}
		</div>
	</div>
</div>
