<script lang="ts">
	interface Props {
		count?: number;
	}

	let { count = 4 }: Props = $props();
</script>

<div
	class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4"
	aria-busy="true"
	aria-label="Loading files"
>
	{#each Array(count) as _}
		<div class="card bg-base-100 shadow-sm">
			<div class="card-body p-3 lg:p-4">
				<div class="flex items-center gap-2 lg:gap-3">
					<div class="h-8 w-8 animate-pulse rounded bg-base-300 lg:h-10 lg:w-10"></div>
					<div class="flex-1 space-y-2">
						<div class="h-4 w-3/4 animate-pulse rounded bg-base-300"></div>
						<div class="h-3 w-1/2 animate-pulse rounded bg-base-300"></div>
					</div>
				</div>
			</div>
		</div>
	{/each}
</div>
