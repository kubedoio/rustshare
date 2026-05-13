<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import {
		FileText,
		Plus,
		Clock,
		Folder,
		MoreHorizontal,
		Search,
		List,
		Grid3X3,
		ArrowUpDown
	} from 'lucide-svelte';

	import { listNotes, createNote, deleteNote } from '$lib/api/notes';
	import { activityStore } from '$lib/stores/activity';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	const notesQuery = createQuery({
		queryKey: ['notes', module.key],
		queryFn: () => listNotes()
	});

	$: recentNotes = $notesQuery.data ?? [];

	let createError = '';
	let isCreating = false;
	let searchTerm = '';
	let statusFilter: 'all' | 'public' | 'private' = 'all';
	let sortDirection: 'desc' | 'asc' = 'desc';
	let viewMode: 'list' | 'grid' = module.ui.page.layout === 'gallery-grid' ? 'grid' : 'list';
	let itemsPerPage = 20;

	$: filteredNotes = recentNotes
		.filter((note) =>
			(note.metadata?.title || note.name || '').toLowerCase().includes(searchTerm.trim().toLowerCase())
		)
		.filter((note) => statusFilter === 'all' || note.metadata?.visibility === statusFilter)
		.sort((a, b) => {
			const aTime = new Date(a.modified_at ?? a.metadata?.updated_at ?? 0).getTime();
			const bTime = new Date(b.modified_at ?? b.metadata?.updated_at ?? 0).getTime();
			return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
		});
	$: visibleNotes = filteredNotes.slice(0, itemsPerPage);
	$: filterLabel =
		statusFilter === 'public' ? 'Public notes' : statusFilter === 'private' ? 'Private notes' : module.ui.page.filterLabel ?? 'All notes';
	$: sortLabel = sortDirection === 'desc' ? 'Modified' : 'Oldest first';

	async function handleNewNote() {
		if (isCreating) return;
		isCreating = true;
		createError = '';

		let title = 'Untitled Note';
		const existingTitles = recentNotes.map((n) => n.metadata?.title?.toLowerCase() ?? n.name?.toLowerCase() ?? '');
		if (existingTitles.includes(title.toLowerCase())) {
			let counter = 2;
			while (existingTitles.includes(`${title} ${counter}`.toLowerCase())) {
				counter++;
			}
			title = `${title} ${counter}`;
		}

		try {
			const result = await createNote({
				title,
				content: `# ${title}\n\n`
			});
			activityStore.addActivity('note_created', result.name || title || 'Untitled Note', {
				artifactId: result.id,
				moduleKey: 'notes'
			});
			goto(`/modules/${module.key}/${result.id}`);
			$notesQuery.refetch();
		} catch (err) {
			console.error('Failed to create note:', err);
			createError = err instanceof Error ? err.message : 'Failed to create note';
		} finally {
			isCreating = false;
		}
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No notes yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ??
		'No notes yet. Create your first note to capture ideas, documentation, or working knowledge.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New note';
	$: searchPlaceholder = module.ui.page.searchPlaceholder ?? 'Search notes...';
	$: itemPlural = module.ui.page.itemPlural ?? 'notes';
</script>

<ModulePageShell title="Notes" subtitle="Write and keep file-backed notes in your workspace.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleNewNote}
			disabled={isCreating || !module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New note</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if createError}
			<div class="rounded-lg border border-red-300 bg-red-50 px-4 py-2 text-sm text-red-700">
				{createError}
			</div>
		{/if}
		{#if $notesQuery.isLoading}
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if recentNotes.length === 0}
			<EmptyState
				icon={"📝"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleNewNote}
			/>
		{:else}
			<div class="overflow-hidden rounded-xl border border-base-300/60 bg-base-100">
				<div class="flex flex-col gap-3 border-b border-base-200 p-3 lg:flex-row lg:items-center">
					<label class="relative min-w-0 flex-1">
						<Search size={16} class="absolute top-1/2 left-3 -translate-y-1/2 text-base-content/35" />
						<input
							class="input-bordered input input-sm w-full pl-9"
							placeholder={searchPlaceholder}
							bind:value={searchTerm}
						/>
					</label>
					<select class="select-bordered select select-sm lg:w-40" bind:value={statusFilter} aria-label="Filter notes">
						<option value="all">{module.ui.page.filterLabel ?? 'All notes'}</option>
						<option value="private">Private notes</option>
						<option value="public">Public notes</option>
					</select>
					<div class="ml-auto flex items-center gap-2">
						<button
							class="btn gap-2 btn-sm btn-outline"
							onclick={() => (sortDirection = sortDirection === 'desc' ? 'asc' : 'desc')}
						>
							<ArrowUpDown size={14} />
							<span>{sortLabel}</span>
						</button>
						<div class="join">
							<button
								class="btn join-item btn-sm {viewMode === 'list' ? 'btn-primary' : 'btn-outline'}"
								aria-label="List view"
								onclick={() => (viewMode = 'list')}
							>
								<List size={15} />
							</button>
							<button
								class="btn join-item btn-sm {viewMode === 'grid' ? 'btn-primary' : 'btn-outline'}"
								aria-label="Grid view"
								onclick={() => (viewMode = 'grid')}
							>
								<Grid3X3 size={15} />
							</button>
						</div>
					</div>
				</div>

				{#if viewMode === 'grid'}
					<div class="grid gap-3 p-3 sm:grid-cols-2 xl:grid-cols-3">
						{#each visibleNotes as note}
							<a href={`/modules/${module.key}/${note.id}`} class="rounded-xl border border-base-300/50 p-4 transition-colors hover:border-brand-500/30 hover:bg-base-200/30">
								<div class="mb-3 flex h-9 w-9 items-center justify-center rounded-lg bg-base-200 text-base-content/55">
									<FileText size={16} />
								</div>
								<p class="truncate text-sm font-medium text-base-content">{(note.metadata?.title || note.name || '').replace(/\.md$/i, '')}</p>
								<p class="mt-1 line-clamp-2 text-xs text-base-content/55">{note.metadata?.excerpt || 'No preview available'}</p>
							</a>
						{/each}
					</div>
				{:else}
					<div class="divide-y divide-base-200">
						{#each visibleNotes as note}
							<a
								href={`/modules/${module.key}/${note.id}`}
								class="flex items-center gap-4 px-4 py-3 transition-colors hover:bg-base-200/40"
							>
								<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-base-200 text-base-content/55">
									<FileText size={16} />
								</div>
								<div class="flex min-w-0 flex-1 flex-col">
									<span class="truncate text-sm font-medium text-base-content">
										{(note.metadata?.title || note.name || '').replace(/\.md$/i, '')}
									</span>
									<span class="line-clamp-1 text-xs text-base-content/55">
										{note.metadata?.excerpt || 'No preview available'}
									</span>
								</div>
								<span class="hidden text-xs text-base-content/55 sm:block">
									{note.modified_at ? new Date(note.modified_at).toLocaleDateString() : ''}
								</span>
								<MoreHorizontal size={16} class="text-base-content/45" />
							</a>
						{/each}
					</div>
				{/if}

				<div class="flex items-center justify-between border-t border-base-200 px-4 py-3 text-sm text-base-content/60">
					<span>{filteredNotes.length} {itemPlural}</span>
					<label class="flex items-center gap-2">
						<span>Items per page</span>
						<select class="select-bordered select select-sm w-20" bind:value={itemsPerPage}>
							<option value={20}>20</option>
							<option value={50}>50</option>
						</select>
					</label>
				</div>
			</div>
		{/if}
	</div>
</ModulePageShell>
