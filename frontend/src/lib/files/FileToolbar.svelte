<script lang="ts">
	import { fileSortState, setSortField, setSortOrder, setViewMode, type SortField, type SortOrder } from '$lib/stores/fileSort';
	import { selectionCount, hasSelection } from '$lib/stores/selection';

	export let selectionMode = false;
	export let onToggleSelection: () => void;
	export let onSelectAll: () => void;
	export let onDeselectAll: () => void;
	export let onBulkDelete: () => void;
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
</script>

<div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
	<!-- Left: Title -->
	<div class="flex items-center gap-4">
		<h1 class="text-xl font-semibold text-[#e5e7eb]">All files</h1>
	</div>

	<!-- Right: Actions -->
	<div class="flex items-center gap-2">
		{#if selectionMode}
			<!-- Selection mode toolbar -->
			<div class="flex items-center gap-2 bg-[#1a1d24] rounded-lg px-3 py-1.5">
				<span class="text-sm font-medium text-[#e5e7eb]">
					{$selectionCount} selected
				</span>
				<div class="w-px h-4 bg-[#2a2f35] mx-1"></div>
				<button
					type="button"
					class="text-sm text-[#9ca3af] hover:text-[#e5e7eb] transition-colors"
					on:click={onSelectAll}
				>
					All
				</button>
				<button
					type="button"
					class="text-sm text-[#9ca3af] hover:text-[#e5e7eb] transition-colors"
					on:click={onDeselectAll}
				>
					None
				</button>
				<button
					type="button"
					class="text-sm text-[#ef4444] hover:text-[#f87171] transition-colors"
					on:click={onBulkDelete}
					disabled={!$hasSelection}
				>
					Delete
				</button>
				<div class="w-px h-4 bg-[#2a2f35] mx-1"></div>
				<button
					type="button"
					class="text-sm text-[#9ca3af] hover:text-[#e5e7eb] transition-colors"
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
					class="flex items-center gap-2 px-3 py-2 text-sm font-medium text-[#9ca3af] hover:text-[#e5e7eb] hover:bg-[#1a1d24] rounded-lg transition-colors"
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
					<div class="absolute right-0 top-full mt-2 w-48 bg-[#181b21] rounded-xl shadow-lg shadow-black/20 border border-[#2a2f35] py-1 z-50">
						{#each sortOptions as option}
							<button
								type="button"
								class="w-full flex items-center justify-between px-4 py-2 text-sm text-left transition-colors
									{$fileSortState.field === option.value ? 'text-[#2563eb] bg-[#2563eb]/10' : 'text-[#9ca3af] hover:bg-[#1a1d24]'}"
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
						<div class="border-t border-[#2a2f35] my-1"></div>
						<button
							type="button"
							class="w-full flex items-center gap-2 px-4 py-2 text-sm text-[#9ca3af] hover:bg-[#1a1d24] transition-colors"
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
			<div class="flex items-center bg-[#1a1d24] rounded-lg p-1">
				<button
					type="button"
					class="p-1.5 rounded-md transition-all
						{$fileSortState.viewMode === 'grid' ? 'bg-[#0f1115] text-[#e5e7eb] shadow-sm' : 'text-[#6b7280] hover:text-[#e5e7eb]'}"
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
						{$fileSortState.viewMode === 'list' ? 'bg-[#0f1115] text-[#e5e7eb] shadow-sm' : 'text-[#6b7280] hover:text-[#e5e7eb]'}"
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

			<div class="w-px h-6 bg-[#2a2f35] mx-1"></div>

			<!-- New Folder button -->
			<button
				type="button"
				class="hidden sm:flex items-center gap-2 px-3 py-2 text-sm font-medium text-[#e5e7eb] hover:bg-[#1a1d24] rounded-lg transition-colors"
				on:click={onNewFolder}
				disabled={isUploading}
			>
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
					<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
					<line x1="12" x2="12" y1="10" y2="16"/>
					<line x1="9" x2="15" y1="13" y2="13"/>
				</svg>
				<span>New folder</span>
			</button>

			<!-- Upload button -->
			<button
				type="button"
				class="flex items-center gap-2 px-4 py-2 text-sm font-medium bg-[#2563eb] hover:bg-[#1d4ed8] text-white rounded-lg transition-colors shadow-sm shadow-[#2563eb]/20"
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

			<!-- Selection mode button -->
			<button
				type="button"
				class="p-2 text-[#6b7280] hover:text-[#e5e7eb] hover:bg-[#1a1d24] rounded-lg transition-colors"
				on:click={onToggleSelection}
				aria-label="Select multiple"
			>
				<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
					<path d="M9 12l2 2 4-4"/>
					<circle cx="12" cy="12" r="10"/>
				</svg>
			</button>
		{/if}
	</div>
</div>

<!-- Click outside to close sort menu -->
{#if sortMenuOpen}
	<div class="fixed inset-0 z-40" on:click={() => sortMenuOpen = false}></div>
{/if}
