<script lang="ts">
	const rows = Array.from({ length: 8 });
</script>

<div
	class="relative overflow-x-auto rounded-xl border border-base-300 bg-base-100"
	aria-busy="true"
	aria-label="Loading file list"
>
	<table class="w-full">
		<thead>
			<tr class="border-b border-base-300 bg-base-200/50">
				<th class="w-10 px-4 py-2 text-left"></th>
				<th
					class="w-12 px-2 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase"
					>Preview</th
				>
				<th
					class="px-4 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase"
					>Name</th
				>
				<th
					class="hidden px-4 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase md:table-cell"
					>Type</th
				>
				<th
					class="hidden px-4 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase sm:table-cell"
					>Size</th
				>
				<th
					class="hidden px-4 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase lg:table-cell"
					>Modified</th
				>
				<th
					class="hidden px-4 py-2 text-left text-meta font-semibold tracking-wider text-base-content/60 uppercase xl:table-cell"
					>Status</th
				>
				<th class="w-10 px-4 py-2"></th>
			</tr>
		</thead>
		<tbody class="divide-y divide-base-300/40">
			{#each rows as _, i (i)}
				<tr class="animate-pulse">
					<td class="w-10 px-4 py-3">
						<div class="h-4 w-4 rounded bg-base-200"></div>
					</td>
					<td class="w-12 px-2 py-3">
						<div class="h-8 w-8 rounded bg-base-200"></div>
					</td>
					<td class="px-4 py-3">
						<div class="h-4 w-32 rounded bg-base-200"></div>
					</td>
					<td class="hidden px-4 py-3 md:table-cell">
						<div class="h-4 w-16 rounded bg-base-200"></div>
					</td>
					<td class="hidden px-4 py-3 sm:table-cell">
						<div class="h-4 w-20 rounded bg-base-200"></div>
					</td>
					<td class="hidden px-4 py-3 lg:table-cell">
						<div class="h-4 w-24 rounded bg-base-200"></div>
					</td>
					<td class="hidden px-4 py-3 xl:table-cell">
						<div class="h-4 w-16 rounded bg-base-200"></div>
					</td>
					<td class="w-10 px-4 py-3">
						<div class="h-4 w-4 rounded bg-base-200"></div>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>
