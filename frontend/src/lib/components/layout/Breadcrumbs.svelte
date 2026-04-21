<script lang="ts">
	import type { Folder } from '$lib/api/types';
	import { Home, ChevronRight } from 'lucide-svelte';

	interface Props {
		folderPath?: Folder[];
		rootLabel?: string;
		onNavigate?: (payload: { folderId: string | null }) => void;
	}

	let {
		folderPath = [],
		rootLabel = 'My Files',
		onNavigate = () => {}
	}: Props = $props();

	function handleNavigate(folderId: string | null) {
		onNavigate({ folderId });
	}
</script>

<nav aria-label="Breadcrumb" class="flex items-center min-w-0">
	<ol class="flex items-center flex-wrap gap-0.5 min-w-0">
		<!-- My Files (Root) -->
		<li class="flex items-center flex-shrink-0">
			<button
				type="button"
				class="flex items-center gap-1.5 px-2 py-1 text-sm font-medium text-base-content/70 hover:text-brand-600 hover:bg-brand-500/10 rounded-md transition-colors"
				onclick={() => handleNavigate(null)}
				aria-label="My Files"
			>
				<Home size={14} />
				<span>{rootLabel}</span>
			</button>
		</li>

		{#each folderPath as folder, index}
			{@const isLast = index === folderPath.length - 1}
			<li class="flex items-center min-w-0">
				<ChevronRight size={14} class="text-base-content/30 mx-0.5 flex-shrink-0" />
				{#if isLast}
					<!-- Current folder - not clickable, visually distinct -->
					<span 
						class="px-2 py-1 text-sm font-semibold text-base-content bg-base-200/70 rounded-md truncate max-w-[120px] sm:max-w-[180px] md:max-w-[240px] lg:max-w-[320px]"
						aria-current="page"
						title={folder.name}
					>
						{folder.name}
					</span>
				{:else}
					<!-- Parent folder - clickable -->
					<button
						type="button"
						class="px-2 py-1 text-sm font-medium text-base-content/70 hover:text-brand-600 hover:bg-brand-500/10 rounded-md transition-colors truncate max-w-[100px] sm:max-w-[150px] md:max-w-[200px]"
						onclick={() => handleNavigate(folder.id)}
						title={folder.name}
					>
						{folder.name}
					</button>
				{/if}
			</li>
		{/each}
	</ol>
</nav>
