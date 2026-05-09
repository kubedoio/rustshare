<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { FileText, Plus, Clock, Folder, MoreHorizontal, Trash2 } from 'lucide-svelte';

	import { listNotes, createNote, deleteNote } from '$lib/api/notes';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: isGallery = module.ui.page.layout === 'gallery-grid';

	const notesQuery = createQuery({
		queryKey: ['notes', module.key],
		queryFn: () => listNotes()
	});

	$: recentNotes = $notesQuery.data ?? [];

	let createError = '';

	async function handleNewNote() {
		createError = '';

		let title = 'Untitled Note';
		const existingNames = recentNotes.map((n) => n.name?.toLowerCase() ?? '');
		if (existingNames.includes(title.toLowerCase())) {
			let counter = 2;
			while (existingNames.includes(`${title} ${counter}`.toLowerCase())) {
				counter++;
			}
			title = `${title} ${counter}`;
		}

		try {
			const result = await createNote({
				title,
				content: `# ${title}\n\n`
			});
			goto(`/modules/${module.key}/${result.id}`);
			$notesQuery.refetch();
		} catch (err) {
			console.error('Failed to create note:', err);
			createError = err instanceof Error ? err.message : 'Failed to create note';
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
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Note';
</script>

<ModulePageShell title="Notes" subtitle="Write and keep file-backed notes in your workspace.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleNewNote}
			disabled={!module.defaultTemplate}
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
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each recentNotes as note}
					<a
						href={`/modules/${module.key}/${note.id}`}
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{note.name.replace(/\.md$/i, '')}</span>
							{#if note.metadata?.excerpt}
								<span class="line-clamp-1 text-xs text-base-content/50">{note.metadata.excerpt}</span>
							{/if}
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{note.modified_at ? new Date(note.modified_at).toLocaleDateString() : ''}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-2">
				{#each recentNotes as note}
					<a
						href={`/modules/${module.key}/${note.id}`}
						class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
					>
						<div
							class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={16} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{note.name.replace(/\.md$/i, '')}</span>
							{#if note.metadata?.excerpt}
								<span class="line-clamp-1 text-xs text-base-content/50">{note.metadata.excerpt}</span>
							{/if}
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{note.modified_at ? new Date(note.modified_at).toLocaleDateString() : ''}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>
