<script lang="ts">
	import type { Folder } from '$lib/api/types';
	import { fileBrowserUi, viewMode } from '$lib/stores/fileBrowserUi';
	import { fileSortState, setSortField } from '$lib/stores/fileSort';
	import { selectionCount, hasSelection, selectionStore } from '$lib/stores/selection';
	import Breadcrumbs from '$lib/components/layout/Breadcrumbs.svelte';
	import {
		ArrowUpDown,
		Grid3x2 as Grid3X3,
		List,
		FolderPlus,
		Upload,
		SquareCheck as CheckSquare,
		X,
		Download,
		Move,
		Trash2,
		ArrowUp,
		ArrowDown
	} from 'lucide-svelte';

	interface Props {
		title?: string;
		description?: string;
		breadcrumbItems?: Folder[];
		rootLabel?: string;
		showBreadcrumbs?: boolean;
		canCreateFolder?: boolean;
		canUpload?: boolean;
		allowSelectionMode?: boolean;
		selectionMode?: boolean;
		viewMode?: 'grid' | 'list';
		searchTerm?: string;
		selectedCount?: number;
		onViewModeChange?: (mode: 'grid' | 'list') => void;
		onSearch?: (term: string) => void;
		onToggleSelection?: () => void;
		onSelectAll?: () => void;
		onDeselectAll?: () => void;
		onBulkDelete?: () => void;
		onBulkDownload?: () => void;
		onBulkMove?: () => void;
		onBulkStar?: () => void;
		onClearSelection?: () => void;
		onNewFolder?: () => void;
		onUpload?: () => void;
		onAsk?: () => void;
		onBreadcrumbNavigate?: (event: CustomEvent<{ folderId: string | null }>) => void;
		isUploading?: boolean;
	}

	let {
		title = 'All files',
		description = '',
		breadcrumbItems = [],
		rootLabel = 'My Files',
		showBreadcrumbs = true,
		canCreateFolder = true,
		canUpload = true,
		allowSelectionMode = true,
		selectionMode = false,
		viewMode: viewModeProp = undefined,
		searchTerm = '',
		selectedCount: selectedCountProp = undefined,
		onViewModeChange = () => {},
		onSearch = () => {},
		onToggleSelection = () => {},
		onSelectAll = () => {},
		onDeselectAll = () => {},
		onBulkDelete = () => {},
		onBulkDownload = () => {},
		onBulkMove = () => {},
		onBulkStar = () => {},
		onClearSelection = () => {},
		onNewFolder = () => {},
		onUpload = () => {},
		onAsk,
		onBreadcrumbNavigate = () => {},
		isUploading = false
	}: Props = $props();

	let sortMenuOpen = $state(false);

	const sortOptions = [
		{ value: 'name', label: 'Name' },
		{ value: 'modified_at', label: 'Date modified' },
		{ value: 'size', label: 'Size' },
		{ value: 'mime_type', label: 'Type' }
	] as const;

	let selectedFileCount = $derived($selectionStore.selectedFileIds.size);
	let selectedFolderCount = $derived($selectionStore.selectedFolderIds.size);
	let effectiveViewMode = $derived(viewModeProp ?? $viewMode ?? 'list');
	let effectiveSelectedCount = $derived(selectedCountProp ?? $selectionCount);

	function handleSortClick(field: (typeof sortOptions)[number]['value']) {
		setSortField(field);
		sortMenuOpen = false;
	}

	function handleBreadcrumbNavigate(payload: { folderId: string | null }) {
		onBreadcrumbNavigate(new CustomEvent('navigate', { detail: payload }));
	}
</script>

<!-- Toolbar -->
<div class="border-b border-base-300/50 px-3 py-2 md:px-4 lg:px-5">
	<div class="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
		<!-- Left: Title and description -->
		<div class="min-w-0">
			<h1 class="text-title-lg truncate font-bold text-base-content">{title}</h1>
			{#if description}
				<p class="mt-0.5 truncate text-body-sm text-base-content/50">{description}</p>
			{/if}
		</div>

		<!-- Right: Actions -->
		<div class="flex flex-wrap items-center gap-2">
			{#if selectionMode}
				<!-- Selection mode toolbar -->
				<div
					class="flex items-center gap-1.5 rounded-lg border border-base-300/60 bg-base-100 px-2.5 py-1.5 shadow-sm"
				>
					<div class="mr-1 flex items-center gap-2">
						<span class="text-sm font-medium text-base-content"
							>{effectiveSelectedCount} selected</span
						>
					</div>
					<div class="h-4 w-px bg-base-300"></div>
					<button
						type="button"
						class="rounded-md p-1 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content"
						onclick={onSelectAll}
						aria-label="Select all"
						title="Select all"
					>
						<CheckSquare size={14} />
					</button>
					<button
						type="button"
						class="rounded-md p-1 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content"
						onclick={onDeselectAll}
						aria-label="Deselect all"
						title="Deselect all"
					>
						<X size={14} />
					</button>
					<div class="h-4 w-px bg-base-300"></div>
					<button
						type="button"
						class="rounded-md p-1 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content disabled:opacity-30"
						onclick={onBulkDownload}
						disabled={selectedFileCount === 0}
						aria-label="Download selected"
						title="Download selected"
					>
						<Download size={14} />
					</button>
					<button
						type="button"
						class="rounded-md p-1 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content disabled:opacity-30"
						onclick={onBulkMove}
						disabled={selectedFileCount === 0 || selectedFolderCount > 0}
						aria-label="Move selected"
						title="Move selected"
					>
						<Move size={14} />
					</button>
					<button
						type="button"
						class="rounded-md p-1 text-error transition-colors hover:bg-error/10 disabled:opacity-30"
						onclick={onBulkDelete}
						disabled={!$hasSelection}
						aria-label="Delete selected"
						title="Delete selected"
					>
						<Trash2 size={14} />
					</button>
					{#if onBulkStar}
						<button
							type="button"
							class="rounded-md p-1 text-base-content/60 transition-colors hover:bg-base-200 hover:text-base-content disabled:opacity-30"
							onclick={onBulkStar}
							disabled={!$hasSelection}
							aria-label="Star selected"
							title="Star selected"
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="2"
								stroke-linecap="round"
								stroke-linejoin="round"
								class="h-3.5 w-3.5"
								><polygon
									points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"
								/></svg
							>
						</button>
					{/if}
					<div class="h-4 w-px bg-base-300"></div>
					<button
						type="button"
						class="px-1 text-xs font-medium text-base-content/60 transition-colors hover:text-base-content"
						onclick={onToggleSelection}
					>
						Done
					</button>
				</div>
			{:else}
				{#if onAsk}
					<button
						type="button"
						class="flex items-center gap-2 rounded-lg border border-brand-500/30 px-2.5 py-1.5 text-sm font-medium text-brand-600 transition-colors hover:bg-brand-500/10"
						onclick={onAsk}
					>
						<span aria-hidden="true">✦</span> Ask this folder
					</button>
				{/if}
				<!-- Sort dropdown -->
				<div class="relative">
					<button
						type="button"
						class="flex items-center gap-2 rounded-lg border border-base-300/60 bg-base-100 px-2.5 py-1.5 text-sm font-medium text-base-content/70 transition-colors hover:border-brand-500/30 hover:text-base-content"
						onclick={() => (sortMenuOpen = !sortMenuOpen)}
						aria-expanded={sortMenuOpen}
						aria-haspopup="listbox"
					>
						<ArrowUpDown size={16} />
						<span class="hidden sm:inline">
							{sortOptions.find((o) => o.value === $fileSortState.field)?.label}
						</span>
						{#if $fileSortState.order === 'asc'}
							<ArrowUp size={14} class="text-base-content/40" />
						{:else}
							<ArrowDown size={14} class="text-base-content/40" />
						{/if}
					</button>

					{#if sortMenuOpen}
						<div
							class="absolute top-full right-0 z-50 mt-2 w-44 rounded-lg border border-base-300/60 bg-base-100 py-1 shadow-xl shadow-black/20"
							role="listbox"
						>
							{#each sortOptions as option}
								<button
									type="button"
									class="flex w-full items-center justify-between px-3 py-2 text-left text-sm transition-colors
										{$fileSortState.field === option.value
										? 'bg-brand-500/10 text-brand-600'
										: 'text-base-content/80 hover:bg-base-200/60'}"
									onclick={() => handleSortClick(option.value)}
									role="option"
									aria-selected={$fileSortState.field === option.value}
								>
									<span>{option.label}</span>
									{#if $fileSortState.field === option.value}
										{#if $fileSortState.order === 'asc'}
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
						class="rounded-md p-1 transition-all
							{effectiveViewMode === 'grid'
							? 'bg-brand-500/10 text-brand-600'
							: 'text-base-content/50 hover:bg-base-200/60 hover:text-base-content'}"
						onclick={() => {
							if (viewModeProp !== undefined) {
								onViewModeChange('grid');
							} else {
								fileBrowserUi.setViewMode('grid');
							}
						}}
						aria-label="Grid view"
						title="Grid view"
					>
						<Grid3X3 size={16} />
					</button>
					<button
						type="button"
						class="rounded-md p-1 transition-all
							{effectiveViewMode === 'list'
							? 'bg-brand-500/10 text-brand-600'
							: 'text-base-content/50 hover:bg-base-200/60 hover:text-base-content'}"
						onclick={() => {
							if (viewModeProp !== undefined) {
								onViewModeChange('list');
							} else {
								fileBrowserUi.setViewMode('list');
							}
						}}
						aria-label="List view"
						title="List view"
					>
						<List size={16} />
					</button>
				</div>

				<div class="hidden h-6 w-px bg-base-300/60 sm:block"></div>

				<!-- New Folder button -->
				{#if canCreateFolder}
					<button
						type="button"
						class="flex items-center gap-2 rounded-lg border border-base-300/60 bg-base-100 px-2.5 py-1.5 text-sm font-medium text-base-content/80 transition-colors hover:border-brand-500/30 hover:text-base-content disabled:opacity-50"
						onclick={onNewFolder}
						disabled={isUploading}
					>
						<FolderPlus size={14} />
						<span class="hidden sm:inline">New folder</span>
					</button>
				{/if}

				<!-- Upload button -->
				{#if canUpload}
					<button
						type="button"
						class="flex items-center gap-2 rounded-lg bg-brand-500 px-3 py-1.5 text-sm font-medium text-white shadow-sm shadow-brand-500/20 transition-colors hover:bg-brand-600 disabled:opacity-50"
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
						class="rounded-lg border border-base-300/60 bg-base-100 p-1.5 text-base-content/60 transition-colors hover:border-brand-500/30 hover:text-base-content"
						onclick={onToggleSelection}
						aria-label="Select multiple"
						title="Select multiple"
					>
						<CheckSquare size={16} />
					</button>
				{/if}
			{/if}
		</div>
	</div>
</div>

<!-- Breadcrumbs -->
{#if showBreadcrumbs && breadcrumbItems.length > 0}
	<div class="border-b border-base-300/50 bg-base-200/30 px-3 py-1.5 md:px-4 lg:px-5">
		<Breadcrumbs folderPath={breadcrumbItems} {rootLabel} onNavigate={handleBreadcrumbNavigate} />
	</div>
{/if}

<!-- Click outside to close sort menu -->
{#if sortMenuOpen}
	<button
		type="button"
		class="fixed inset-0 z-40 cursor-default"
		aria-label="Close sort menu"
		onclick={() => (sortMenuOpen = false)}
	></button>
{/if}
