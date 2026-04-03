<script lang="ts">
	import { fileBrowserUi, viewMode, sortField, sortOrder } from '$lib/stores/fileBrowserUi';
	import { selectionCount, hasSelection, selectionStore } from '$lib/stores/selection';
	import { ArrowUpDown, Grid3x2 as Grid3X3, List, FolderPlus, Upload, SquareCheck as CheckSquare, X, Download, Move, Trash2, ArrowUp, ArrowDown } from 'lucide-svelte';

	// Props
	interface Props {
		title?: string;
		description?: string;
		canCreateFolder?: boolean;
		canUpload?: boolean;
		allowSelectionMode?: boolean;
		selectionMode?: boolean;
		onToggleSelection?: () => void;
		onSelectAll?: () => void;
		onDeselectAll?: () => void;
		onBulkDelete?: () => void;
		onBulkDownload?: () => void;
		onBulkMove?: () => void;
		onNewFolder?: () => void;
		onUpload?: () => void;
		isUploading?: boolean;
	}

	let {
		title = 'All files',
		description = '',
		canCreateFolder = true,
		canUpload = true,
		allowSelectionMode = true,
		selectionMode = false,
		onToggleSelection = () => {},
		onSelectAll = () => {},
		onDeselectAll = () => {},
		onBulkDelete = () => {},
		onBulkDownload = () => {},
		onBulkMove = () => {},
		onNewFolder = () => {},
		onUpload = () => {},
		isUploading = false
	}: Props = $props();

	let sortMenuOpen = $state(false);

	const sortOptions = [
		{ value: 'name', label: 'Name' },
		{ value: 'modified_at', label: 'Date modified' },
		{ value: 'size', label: 'Size' },
		{ value: 'type', label: 'Type' },
	] as const;

	let selectedFileCount = $derived($selectionStore.selectedFileIds.size);
	let selectedFolderCount = $derived($selectionStore.selectedFolderIds.size);

	function handleSortClick(field: typeof sortOptions[number]['value']) {
		fileBrowserUi.toggleSort(field);
		sortMenuOpen = false;
	}
</script>

<div class="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
	<!-- Left: Title and description -->
	<div class="min-w-0">
		<h1 class="text-lg font-semibold text-base-content truncate">{title}</h1>
		{#if description}
			<p class="text-sm text-base-content/50 mt-0.5 truncate">{description}</p>
		{/if}
	</div>

	<!-- Right: Actions -->
	<div class="flex flex-wrap items-center gap-2">
		{#if selectionMode}
			<!-- Selection mode toolbar -->
			<div class="flex items-center gap-2 rounded-lg border border-base-300/60 bg-base-100 px-3 py-2 shadow-sm">
				<div class="flex items-center gap-2 mr-1">
					<span class="text-sm font-medium text-base-content">{$selectionCount} selected</span>
				</div>
				<div class="w-px h-4 bg-base-300"></div>
				<button
					type="button"
					class="p-1.5 rounded-md text-base-content/60 hover:bg-base-200 hover:text-base-content transition-colors"
					onclick={onSelectAll}
					aria-label="Select all"
					title="Select all"
				>
					<CheckSquare size={16} />
				</button>
				<button
					type="button"
					class="p-1.5 rounded-md text-base-content/60 hover:bg-base-200 hover:text-base-content transition-colors"
					onclick={onDeselectAll}
					aria-label="Deselect all"
					title="Deselect all"
				>
					<X size={16} />
				</button>
				<div class="w-px h-4 bg-base-300"></div>
				<button
					type="button"
					class="p-1.5 rounded-md text-base-content/60 hover:bg-base-200 hover:text-base-content transition-colors disabled:opacity-30"
					onclick={onBulkDownload}
					disabled={selectedFileCount === 0}
					aria-label="Download selected"
					title="Download selected"
				>
					<Download size={16} />
				</button>
				<button
					type="button"
					class="p-1.5 rounded-md text-base-content/60 hover:bg-base-200 hover:text-base-content transition-colors disabled:opacity-30"
					onclick={onBulkMove}
					disabled={selectedFileCount === 0 || selectedFolderCount > 0}
					aria-label="Move selected"
					title="Move selected"
				>
					<Move size={16} />
				</button>
				<button
					type="button"
					class="p-1.5 rounded-md text-error hover:bg-error/10 transition-colors disabled:opacity-30"
					onclick={onBulkDelete}
					disabled={!$hasSelection}
					aria-label="Delete selected"
					title="Delete selected"
				>
					<Trash2 size={16} />
				</button>
				<div class="w-px h-4 bg-base-300"></div>
				<button
					type="button"
					class="text-sm font-medium text-base-content/60 hover:text-base-content transition-colors px-1"
					onclick={onToggleSelection}
				>
					Done
				</button>
			</div>
		{:else}
			<!-- Sort dropdown -->
			<div class="relative">
				<button
					type="button"
					class="flex items-center gap-2 rounded-lg border border-base-300/60 bg-base-100 px-3 py-2 text-sm font-medium text-base-content/70 transition-colors hover:border-brand-500/30 hover:text-base-content"
					onclick={() => sortMenuOpen = !sortMenuOpen}
					aria-expanded={sortMenuOpen}
					aria-haspopup="listbox"
				>
					<ArrowUpDown size={16} />
					<span class="hidden sm:inline">
						{sortOptions.find(o => o.value === $sortField)?.label}
					</span>
					{#if $sortOrder === 'asc'}
						<ArrowUp size={14} class="text-base-content/40" />
					{:else}
						<ArrowDown size={14} class="text-base-content/40" />
					{/if}
				</button>

				{#if sortMenuOpen}
					<div 
						class="absolute right-0 top-full mt-2 w-44 bg-base-100 rounded-lg shadow-xl shadow-black/20 border border-base-300/60 py-1 z-50"
						role="listbox"
					>
						{#each sortOptions as option}
							<button
								type="button"
								class="w-full flex items-center justify-between px-3 py-2 text-sm text-left transition-colors
									{$sortField === option.value ? 'text-brand-600 bg-brand-500/10' : 'text-base-content/80 hover:bg-base-200/60'}"
								onclick={() => handleSortClick(option.value)}
								role="option"
								aria-selected={$sortField === option.value}
							>
								<span>{option.label}</span>
								{#if $sortField === option.value}
									{#if $sortOrder === 'asc'}
										<ArrowUp size={14} />
									{:else}
										<ArrowDown size={14} />
									{/if}
								{/if}
							</button>
						{/each}
					</div>
				{/if}
			</div>

			<!-- View mode toggle -->
			<div class="flex items-center rounded-lg border border-base-300/60 bg-base-100 p-1">
				<button
					type="button"
					class="p-1.5 rounded-md transition-all
						{$viewMode === 'grid' ? 'bg-brand-500/10 text-brand-600' : 'text-base-content/50 hover:text-base-content hover:bg-base-200/60'}"
					onclick={() => fileBrowserUi.setViewMode('grid')}
					aria-label="Grid view"
					title="Grid view"
				>
					<Grid3X3 size={18} />
				</button>
				<button
					type="button"
					class="p-1.5 rounded-md transition-all
						{$viewMode === 'list' ? 'bg-brand-500/10 text-brand-600' : 'text-base-content/50 hover:text-base-content hover:bg-base-200/60'}"
					onclick={() => fileBrowserUi.setViewMode('list')}
					aria-label="List view"
					title="List view"
				>
					<List size={18} />
				</button>
			</div>

			<div class="w-px h-6 bg-base-300/60 hidden sm:block"></div>

			<!-- New Folder button -->
			{#if canCreateFolder}
				<button
					type="button"
					class="flex items-center gap-2 rounded-lg border border-base-300/60 bg-base-100 px-3 py-2 text-sm font-medium text-base-content/80 transition-colors hover:border-brand-500/30 hover:text-base-content disabled:opacity-50"
					onclick={onNewFolder}
					disabled={isUploading}
				>
					<FolderPlus size={16} />
					<span class="hidden sm:inline">New folder</span>
				</button>
			{/if}

			<!-- Upload button -->
			{#if canUpload}
				<button
					type="button"
					class="flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-medium text-white shadow-sm shadow-brand-500/20 transition-colors hover:bg-brand-600 disabled:opacity-50"
					onclick={onUpload}
					disabled={isUploading}
				>
					<Upload size={16} />
					<span class="hidden sm:inline">Upload</span>
				</button>
			{/if}

			<!-- Selection mode button -->
			{#if allowSelectionMode}
				<button
					type="button"
					class="rounded-lg border border-base-300/60 bg-base-100 p-2 text-base-content/60 transition-colors hover:border-brand-500/30 hover:text-base-content"
					onclick={onToggleSelection}
					aria-label="Select multiple"
					title="Select multiple"
				>
					<CheckSquare size={18} />
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
		onclick={() => sortMenuOpen = false}
	></button>
{/if}
