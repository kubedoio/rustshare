<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { Folder } from '$lib/api/types';
	import { HardDrive, ChevronRight } from 'lucide-svelte';

	export let folderPath: Folder[] = [];

	type DispatchEvents = {
		navigate: { folderId: string | null };
	}
	const dispatch = createEventDispatcher<DispatchEvents>();

	function handleNavigate(folderId: string | null) {
		dispatch('navigate', { folderId });
	}
</script>

<nav aria-label="Breadcrumb" class="flex items-center">
	<ol class="flex items-center flex-wrap gap-1">
		<!-- Home/Root -->
		<li class="flex items-center">
			<button
				type="button"
				class="flex items-center gap-1.5 px-2 py-1 text-sm font-medium text-base-content/70 hover:text-brand-600 hover:bg-brand-500/10 rounded-md transition-colors"
				on:click={() => handleNavigate(null)}
				aria-label="Root"
			>
				<HardDrive size={16} />
				<span>Root</span>
			</button>
		</li>

		{#each folderPath as folder, index}
			{@const isLast = index === folderPath.length - 1}
			<li class="flex items-center">
				<ChevronRight size={16} class="text-base-content/30 mx-1" />
				{#if isLast}
					<!-- Current folder - not clickable -->
					<span 
						class="px-2 py-1 text-sm font-semibold text-base-content bg-base-200/60 rounded-md"
						aria-current="page"
						title={folder.name}
					>
						<span class="truncate max-w-[150px] sm:max-w-[200px] md:max-w-[300px] lg:max-w-[400px] inline-block align-bottom">
							{folder.name}
						</span>
					</span>
				{:else}
					<!-- Parent folder - clickable -->
					<button
						type="button"
						class="px-2 py-1 text-sm font-medium text-base-content/70 hover:text-brand-600 hover:bg-brand-500/10 rounded-md transition-colors truncate max-w-[150px] sm:max-w-[200px] md:max-w-[250px]"
						on:click={() => handleNavigate(folder.id)}
						title={folder.name}
					>
						{folder.name}
					</button>
				{/if}
			</li>
		{/each}
	</ol>
</nav>
