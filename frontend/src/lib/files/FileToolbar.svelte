<script lang="ts">
	import { fileSortState, setSortField, setSortOrder, setViewMode, type SortField, type SortOrder } from '$lib/stores/fileSort';
	import { selectionCount, hasSelection, selectionStore } from '$lib/stores/selection';

	export let title = 'All files';
	export let description = '';
	export let canCreateFolder = true;
	export let canUpload = true;
	export let allowSelectionMode = true;
	export let selectionMode = false;
	export let onToggleSelection: () => void;
	export let onSelectAll: () => void;
	export let onDeselectAll: () => void;
	export let onBulkDelete: () => void;
	export let onBulkDownload: () => void;
	export let onBulkMove: () => void;
	export let onNewFolder: () => void;
	export let onUpload: () => void;
	export let isUploading = false;

	let sortMenuOpen = false;

	const sortOptions: Array<{ value: SortField; label: string }> = [
		{ value: 'name', label: 'Name' },
		{ value: 'modified_at', label: 'Date modified' },
		{ value: 'size', label: 'Size' },
		{ value: 'mime_type', label: 'Type' },
	];

	function toggleSortOrder() {
		const newOrder: SortOrder = $fileSortState.order === 'asc' ? 'desc' : 'asc';
		setSortOrder(newOrder);
	}

	$: selectedFileCount = $selectionStore.selectedFileIds.size;
	$: selectedFolderCount = $selectionStore.selectedFolderIds.size;
</script>

<div class="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
	<!-- Left: Title and filter chips -->
	<div class="space-y-1">
		<div class="flex flex-wrap items-center gap-2 md:gap-3">
			<h1 class="text-xl font-semibold tracking-tight text-base-content">{title}</h1>
		</div>
		{#if description}
			<p class="max-w-2xl text-sm text-base-content/55">{description}</p>
		{/if}
	</div>

	<!-- Right: Actions -->
	<div class="flex flex-wrap items-center justify-end gap-2">
		{#if selectionMode}
			<!-- Selection mode toolbar -->
			<div class="flex flex-wrap items-center gap-2 rounded-xl border border-base-300/70 bg-base-200/80 px-3 py-2">
				<div class="mr-1">
					<span class="text-sm font-medium text-base-content">{$selectionCount} selected</span>
					{#if selectedFileCount || selectedFolderCount}
						<p class="text-xs text-base-content/55">
							{#if selectedFileCount}{selectedFileCount} file{selectedFileCount === 1 ? '' : 's'}{/if}
							{#if selectedFileCount && selectedFolderCount} and {/if}
							{#if selectedFolderCount}{selectedFolderCount} folder{selectedFolderCount === 1 ? '' : 's'}{/if}
						</p>
					{/if}
				</div>
				<div class="w-px h-4 bg-base-300 mx-1"></div>
				<button
					type="button"
					class="text-sm text-base-content/70 hover:text-base-content transition-colors"
					on:click={onSelectAll}
				>
					All
				</button>
				<button
					type="button"
					class="text-sm text-base-content/70 hover:text-base-content transition-colors"
					on:click={onDeselectAll}
				>
					None
				</button>
				<button
					type="button"
					class="text-sm text-base-content/70 transition-colors hover:text-base-content disabled:cursor-not-allowed disabled:text-base-content/30"
					on:click={onBulkDownload}
					disabled={selectedFileCount === 0}
				>
					Download
				</button>
				<button
					type="button"
					class="text-sm text-base-content/70 transition-colors hover:text-base-content disabled:cursor-not-allowed disabled:text-base-content/30"
					on:click={onBulkMove}
					disabled={selectedFileCount === 0 || selectedFolderCount > 0}
				>
					Move
				</button>
				<button
					type="button"
					class="text-sm text-error hover:text-error/80 transition-colors"
					on:click={onBulkDelete}
					disabled={!$hasSelection}
				>
					Delete
				</button>
				<div class="w-px h-4 bg-base-300 mx-1"></div>
				<button
					type="button"
					class="text-sm text-base-content/70 hover:text-base-content transition-colors"
					on:click={onToggleSelection}
				>
					Done
				</button>
			</div>
		{:else}
			<!-- Sort dropdown -->
			<div class="relative">
				<button
					type="button"
					class="flex items-center gap-2 rounded-xl border border-base-300/70 bg-base-200/80 px-3 py-1.5 text-sm font-medium text-base-content/70 transition-colors hover:border-brand-500/20 hover:text-base-content"
					on:click={() => sortMenuOpen = !sortMenuOpen}
					aria-expanded={sortMenuOpen}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
						<path d="m3 16 4 4 4-4"/>
						<path d="M7 20V4"/>
						<path d="m21 8-4-4-4 4"/>
						<path d="M17 4v16"/>
					</svg>
					<span class="hidden sm:inline">Sort</span>
				</button>

				{#if sortMenuOpen}
					<div class="absolute right-0 top-full mt-2 w-48 bg-base-100 rounded-xl shadow-lg shadow-black/20 border border-base-300 py-1 z-50">
						{#each sortOptions as option}
							<button
								type="button"
								class="w-full flex items-center justify-between px-4 py-2 text-sm text-left transition-colors
									{$fileSortState.field === option.value ? 'text-brand-400 bg-brand-500/10' : 'text-base-content/80 hover:bg-base-200'}"
								on:click={() => { setSortField(option.value); sortMenuOpen = false; }}
							>
								{option.label}
								{#if $fileSortState.field === option.value}
									<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
										{#if $fileSortState.order === 'asc'}
											<path d="m18 15-6-6-6 6"/>
										{:else}
											<path d="m6 9 6 6 6-6"/>
										{/if}
									</svg>
								{/if}
							</button>
						{/each}
						<div class="border-t border-base-200 my-1"></div>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-sm text-base-content/80 hover:bg-base-200 transition-colors"
							on:click={() => { toggleSortOrder(); sortMenuOpen = false; }}
						>
							{#if $fileSortState.order === 'asc'}
								<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
									<path d="m3 16 4 4 4-4"/>
									<path d="M7 20V4"/>
									<path d="m21 8-4-4-4 4"/>
									<path d="M17 4v16"/>
								</svg>
								<span>Descending</span>
							{:else}
								<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
									<path d="m3 8 4-4 4 4"/>
									<path d="M7 4v16"/>
									<path d="m21 16-4 4-4-4"/>
									<path d="M17 20V4"/>
								</svg>
								<span>Ascending</span>
							{/if}
						</button>
					</div>
				{/if}
			</div>

			<!-- View mode toggle -->
			<div class="flex items-center rounded-xl border border-base-300/70 bg-base-200/80 p-1">
				<button
					type="button"
					class="p-1.5 rounded-md transition-all
						{$fileSortState.viewMode === 'grid' ? 'bg-base-100 text-base-content shadow-sm' : 'text-base-content/50 hover:text-base-content'}"
					on:click={() => setViewMode('grid')}
					aria-label="Grid view"
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
						<rect width="7" height="7" x="3" y="3" rx="1"/>
						<rect width="7" height="7" x="14" y="3" rx="1"/>
						<rect width="7" height="7" x="14" y="14" rx="1"/>
						<rect width="7" height="7" x="3" y="14" rx="1"/>
					</svg>
				</button>
				<button
					type="button"
					class="p-1.5 rounded-md transition-all
						{$fileSortState.viewMode === 'list' ? 'bg-base-100 text-base-content shadow-sm' : 'text-base-content/50 hover:text-base-content'}"
					on:click={() => setViewMode('list')}
					aria-label="List view"
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
						<line x1="3" x2="21" y1="6" y2="6"/>
						<line x1="3" x2="21" y1="12" y2="12"/>
						<line x1="3" x2="21" y1="18" y2="18"/>
					</svg>
				</button>
			</div>

			<div class="mx-1 hidden h-6 w-px bg-base-300 lg:block"></div>

			<!-- New Folder button -->
			{#if canCreateFolder}
				<button
					type="button"
					class="flex items-center gap-2 rounded-xl border border-base-300/70 bg-base-100 px-3 py-1.5 text-sm font-medium text-base-content/80 transition-colors hover:border-brand-500/20 hover:text-base-content"
					on:click={onNewFolder}
					disabled={isUploading}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
						<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
						<line x1="12" x2="12" y1="10" y2="16"/>
						<line x1="9" x2="15" y1="13" y2="13"/>
					</svg>
					<span class="hidden sm:inline">New folder</span>
				</button>
			{/if}

			<!-- Upload button -->
			{#if canUpload}
				<button
					type="button"
					class="flex items-center gap-2 rounded-xl bg-brand-500 px-4 py-1.5 text-sm font-medium text-white shadow-sm shadow-brand-500/20 transition-colors hover:bg-brand-600"
					on:click={onUpload}
					disabled={isUploading}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
						<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
						<polyline points="17 8 12 3 7 8"/>
						<line x1="12" x2="12" y1="3" y2="15"/>
					</svg>
					<span class="hidden sm:inline">Upload</span>
				</button>
			{/if}

			<!-- Selection mode button -->
			{#if allowSelectionMode}
				<button
					type="button"
					class="rounded-xl border border-base-300/70 bg-base-100 p-1.5 text-base-content/60 transition-colors hover:border-brand-500/20 hover:text-base-content"
					on:click={onToggleSelection}
					aria-label="Select multiple"
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
						<path d="M9 12l2 2 4-4"/>
						<circle cx="12" cy="12" r="10"/>
					</svg>
				</button>
			{/if}
		{/if}
	</div>
</div>

<!-- Click outside to close sort menu -->
{#if sortMenuOpen}
	<button
		type="button"
		class="fixed inset-0 z-40 cursor-default"
		aria-label="Close sort menu"
		on:click={() => sortMenuOpen = false}
	></button>
{/if}
