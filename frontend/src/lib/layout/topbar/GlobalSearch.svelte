<script lang="ts">
	import { Search, X, Folder, FileText } from 'lucide-svelte';

	interface SearchItem {
		id: string;
		name: string;
		path: string;
	}

	export let value: string;
	export let results: { files: SearchItem[]; folders: SearchItem[] } = { files: [], folders: [] };
	export let onChange: (query: string) => void;
	export let onClear: () => void;
	export let onSelect: (type: 'file' | 'folder', id: string) => void;

	function handleInput(event: Event) {
		onChange((event.target as HTMLInputElement).value);
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			onClear();
		}
	}
</script>

<div class="group relative">
	<div class="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
		<Search
			size={16}
			class="text-base-content/30 transition-colors group-focus-within:text-brand-500"
		/>
	</div>
	<input
		type="text"
		placeholder="Search files, folders, or activity..."
		class="w-full rounded-2xl border border-base-300/50 bg-base-200/50 px-10 py-2 text-sm text-base-content transition-all placeholder:text-base-content/30 focus:border-brand-500/50 focus:bg-base-100 focus:ring-4 focus:ring-brand-500/10 focus:outline-hidden"
		{value}
		on:input={handleInput}
		on:keydown={handleKeydown}
		aria-autocomplete="list"
		aria-expanded={value.length > 0}
		aria-controls={value.length > 0 ? 'search-results' : undefined}
	/>
	{#if value}
		<button
			type="button"
			class="absolute inset-y-0 right-0 flex items-center pr-3 text-base-content/30 hover:text-base-content"
			on:click={onClear}
			aria-label="Clear search"
		>
			<X size={16} />
		</button>

		<!-- Global Search Results Dropdown -->
		<div
			id="search-results"
			role="listbox"
			class="animate-in fade-in zoom-in absolute top-full right-0 left-0 z-50 mt-2 overflow-hidden rounded-2xl border border-base-300 bg-base-100 p-2 shadow-2xl ring-1 ring-black/5 duration-100"
		>
			{#if results.folders.length === 0 && results.files.length === 0}
				<div class="py-6 text-center" role="status">
					<p class="text-sm font-medium text-base-content/60">No results found for "{value}"</p>
				</div>
			{:else}
				{#if results.folders.length > 0}
					<div
						class="mb-1 px-2 py-1.5 text-xs font-bold tracking-wider text-base-content/50 uppercase"
					>
						Folders
					</div>
					{#each results.folders as folder (folder.id)}
						<button
							role="option"
							class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-sm transition-colors hover:bg-base-200"
							on:click={() => onSelect('folder', folder.id)}
						>
							<Folder size={16} class="shrink-0 text-brand-500" />
							<div class="flex flex-col items-start truncate leading-tight">
								<span class="truncate font-medium text-base-content">{folder.name}</span>
								<span class="mt-0.5 truncate text-2xs text-base-content/50">{folder.path}</span>
							</div>
						</button>
					{/each}
				{/if}

				{#if results.folders.length > 0 && results.files.length > 0}
					<div class="my-2 h-px w-full bg-base-200"></div>
				{/if}

				{#if results.files.length > 0}
					<div
						class="mb-1 px-2 py-1.5 text-xs font-bold tracking-wider text-base-content/50 uppercase"
					>
						Files
					</div>
					{#each results.files as file (file.id)}
						<button
							role="option"
							class="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-sm transition-colors hover:bg-base-200"
							on:click={() => onSelect('file', file.id)}
						>
							<FileText size={16} class="shrink-0 text-brand-500" />
							<div class="flex flex-col items-start truncate leading-tight">
								<span class="truncate font-medium text-base-content">{file.name}</span>
								<span class="mt-0.5 truncate text-2xs text-base-content/50">{file.path}</span>
							</div>
						</button>
					{/each}
				{/if}
			{/if}
		</div>
	{/if}
</div>
