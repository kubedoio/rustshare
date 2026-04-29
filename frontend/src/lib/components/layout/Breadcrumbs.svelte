<script lang="ts">
	import type { Folder } from '$lib/api/types';
	import { Home, ChevronRight } from 'lucide-svelte';

	interface Props {
		folderPath?: Folder[];
		rootLabel?: string;
		onNavigate?: (payload: { folderId: string | null }) => void;
	}

	let { folderPath = [], rootLabel = 'My Files', onNavigate = () => {} }: Props = $props();

	function handleNavigate(folderId: string | null) {
		onNavigate({ folderId });
	}
</script>

<nav aria-label="Breadcrumb" class="flex min-w-0 items-center">
	<ol class="flex min-w-0 flex-wrap items-center gap-0.5">
		<!-- My Files (Root) -->
		<li class="flex flex-shrink-0 items-center">
			<button
				type="button"
				class="flex items-center gap-1.5 rounded-md px-2 py-1 text-sm font-medium text-base-content/70 transition-colors hover:bg-brand-500/10 hover:text-brand-600"
				onclick={() => handleNavigate(null)}
				aria-label="My Files"
			>
				<Home size={14} />
				<span>{rootLabel}</span>
			</button>
		</li>

		{#each folderPath as folder, index}
			{@const isLast = index === folderPath.length - 1}
			<li class="flex min-w-0 items-center">
				<ChevronRight size={14} class="mx-0.5 flex-shrink-0 text-base-content/30" />
				{#if isLast}
					<!-- Current folder - not clickable, visually distinct -->
					<span
						class="max-w-[120px] truncate rounded-md bg-base-200/70 px-2 py-1 text-sm font-semibold text-base-content sm:max-w-[180px] md:max-w-[240px] lg:max-w-[320px]"
						aria-current="page"
						title={folder.name}
					>
						{folder.name}
					</span>
				{:else}
					<!-- Parent folder - clickable -->
					<button
						type="button"
						class="max-w-[100px] truncate rounded-md px-2 py-1 text-sm font-medium text-base-content/70 transition-colors hover:bg-brand-500/10 hover:text-brand-600 sm:max-w-[150px] md:max-w-[200px]"
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
