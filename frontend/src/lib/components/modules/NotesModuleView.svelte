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
		ArrowUpDown,
		Paperclip,
		Image,
		Pencil,
		FolderInput,
		Copy,
		Trash2
	} from 'lucide-svelte';

	import {
		listNotes,
		createNote,
		deleteNote,
		renameNote,
		moveNote,
		duplicateNote
	} from '$lib/api/notes';
	import { activityStore } from '$lib/stores/activity';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { toastStore } from '$lib/stores/toast';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const notesQuery = createQuery({
		queryKey: ['notes', module.key],
		queryFn: () => listNotes()
	});

	let recentNotes = $derived($notesQuery.data ?? []);

	let createError = $state('');
	let isCreating = $state(false);
	let searchTerm = $state('');
	let statusFilter = $state<'all' | 'public' | 'private'>('all');
	let sortDirection = $state<'desc' | 'asc'>('desc');
	let viewMode = $state<'list' | 'grid'>(
		module.ui.page.layout === 'gallery-grid' ? 'grid' : 'list'
	);
	let itemsPerPage = $state(20);

	let activeNote = $state<any>(null);
	let showRenameModal = $state(false);
	let showMoveModal = $state(false);
	let showDeleteModal = $state(false);
	let renameError = $state('');
	let isRenaming = $state(false);
	let isMoving = $state(false);
	let isDeleting = $state(false);
	let isDuplicating = $state(false);

	let filteredNotes = $derived(
		recentNotes
			.filter((note) =>
				(note.metadata?.title || note.name || '')
					.toLowerCase()
					.includes(searchTerm.trim().toLowerCase())
			)
			.filter((note) => statusFilter === 'all' || note.metadata?.visibility === statusFilter)
			.toSorted((a, b) => {
				const aTime = new Date(a.modified_at ?? a.metadata?.updated_at ?? 0).getTime();
				const bTime = new Date(b.modified_at ?? b.metadata?.updated_at ?? 0).getTime();
				return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
			})
	);
	let visibleNotes = $derived(filteredNotes.slice(0, itemsPerPage));
	let filterLabel = $derived(
		statusFilter === 'public'
			? 'Public notes'
			: statusFilter === 'private'
				? 'Private notes'
				: (module.ui.page.filterLabel ?? 'All notes')
	);
	let sortLabel = $derived(sortDirection === 'desc' ? 'Modified' : 'Oldest first');

	async function handleNewNote() {
		if (isCreating) return;
		isCreating = true;
		createError = '';

		let title = 'Untitled Note';
		const existingTitles = recentNotes.map(
			(n) => n.metadata?.title?.toLowerCase() ?? n.name?.toLowerCase() ?? ''
		);
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

	function handleShowAttachments(note: any) {
		goto(`/modules/notes/${note.id}?attachments=open`);
	}

	function openRenameModal(note: any) {
		activeNote = note;
		showRenameModal = true;
		renameError = '';
	}

	function openMoveModal(note: any) {
		activeNote = note;
		showMoveModal = true;
	}

	function openDeleteModal(note: any) {
		activeNote = note;
		showDeleteModal = true;
	}

	async function handleRenameConfirm(newTitle: string) {
		if (isRenaming || !activeNote) return;
		const trimmed = newTitle.trim();
		if (!trimmed) {
			renameError = 'Title is required';
			return;
		}
		isRenaming = true;
		renameError = '';
		try {
			await renameNote(activeNote.id, { title: trimmed });
			toastStore.show('Note renamed', 'success');
			showRenameModal = false;
			$notesQuery.refetch();
		} catch (err) {
			console.error('Failed to rename note:', err);
			renameError = err instanceof Error ? err.message : 'Failed to rename';
		} finally {
			isRenaming = false;
		}
	}

	async function handleMoveConfirm(payload: { targetFolderId: string | null }) {
		if (isMoving || !activeNote) return;
		isMoving = true;
		try {
			await moveNote(activeNote.id, { target_folder_id: payload.targetFolderId });
			toastStore.show('Note moved', 'success');
			showMoveModal = false;
			$notesQuery.refetch();
		} catch (err) {
			console.error('Failed to move note:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to move', 'error');
		} finally {
			isMoving = false;
		}
	}

	async function handleDuplicate(note: any) {
		if (isDuplicating) return;
		isDuplicating = true;
		try {
			const duplicated = await duplicateNote(note.id);
			toastStore.show('Note duplicated', 'success');
			goto(`/modules/notes/${duplicated.id}`);
		} catch (err) {
			console.error('Failed to duplicate note:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to duplicate note', 'error');
		} finally {
			isDuplicating = false;
		}
	}

	async function handleDeleteConfirm() {
		if (isDeleting || !activeNote) return;
		isDeleting = true;
		try {
			await deleteNote(activeNote.id);
			toastStore.show('Note deleted', 'success');
			showDeleteModal = false;
			$notesQuery.refetch();
		} catch (err) {
			console.error('Failed to delete note:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to delete', 'error');
		} finally {
			isDeleting = false;
		}
	}

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No notes yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No notes yet. Create your first note to capture ideas, documentation, or working knowledge.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New note');
	let searchPlaceholder = $derived(module.ui.page.searchPlaceholder ?? 'Search notes...');
	let itemPlural = $derived(module.ui.page.itemPlural ?? 'notes');
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
				icon={'📝'}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleNewNote}
			/>
		{:else}
			<div class="overflow-hidden rounded-xl border border-base-300/60 bg-base-100">
				<div class="flex flex-col gap-3 border-b border-base-200 p-3 lg:flex-row lg:items-center">
					<label class="relative min-w-0 flex-1">
						<Search
							size={16}
							class="absolute top-1/2 left-3 -translate-y-1/2 text-base-content/35"
						/>
						<input
							class="input-bordered input input-sm w-full pl-9"
							placeholder={searchPlaceholder}
							bind:value={searchTerm}
						/>
					</label>
					<select
						class="select-bordered select select-sm lg:w-40"
						bind:value={statusFilter}
						aria-label="Filter notes"
					>
						<option value="all">{module.ui.page.filterLabel ?? 'All notes'}</option>
						<option value="private">Private notes</option>
						<option value="public">Public notes</option>
					</select>
					<div class="ml-auto flex items-center gap-2">
						<button
							class="btn gap-2 btn-outline btn-sm"
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
							<div
								class="relative rounded-xl border border-base-300/50 p-4 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
							>
								<a href={`/modules/${module.key}/${note.id}`} class="block">
									<div
										class="mb-3 flex h-9 w-9 items-center justify-center rounded-lg bg-base-200 text-base-content/55"
									>
										<FileText size={16} />
									</div>
									<p class="truncate pr-6 text-sm font-medium text-base-content">
										{(note.metadata?.title || note.name || '').replace(/\.md$/i, '')}
									</p>
									<p class="mt-1 line-clamp-2 text-xs text-base-content/55">
										{note.metadata?.excerpt || 'No preview available'}
									</p>
									{#if note.attachment_count || note.drawing_count}
										<div class="mt-2 flex items-center gap-3 text-xs text-base-content/50">
											{#if note.attachment_count}
												<span class="flex items-center gap-1">
													<Paperclip size={12} />
													{note.attachment_count}
												</span>
											{/if}
											{#if note.drawing_count}
												<span class="flex items-center gap-1">
													<Image size={12} />
													{note.drawing_count}
												</span>
											{/if}
										</div>
									{/if}
								</a>
								{@render noteActions(note, 'grid')}
							</div>
						{/each}
					</div>
				{:else}
					<div class="divide-y divide-base-200">
						{#each visibleNotes as note}
							<div class="flex items-center gap-4 px-4 py-3 transition-colors hover:bg-base-200/40">
								<a
									href={`/modules/${module.key}/${note.id}`}
									class="flex min-w-0 flex-1 items-center gap-4"
								>
									<div
										class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-base-200 text-base-content/55"
									>
										<FileText size={16} />
									</div>
									<div class="flex min-w-0 flex-1 flex-col">
										<span class="truncate text-sm font-medium text-base-content">
											{(note.metadata?.title || note.name || '').replace(/\.md$/i, '')}
										</span>
										<span class="line-clamp-1 text-xs text-base-content/55">
											{note.metadata?.excerpt || 'No preview available'}
										</span>
										{#if note.attachment_count || note.drawing_count}
											<div class="mt-1 flex items-center gap-3 text-xs text-base-content/50">
												{#if note.attachment_count}
													<span class="flex items-center gap-1">
														<Paperclip size={12} />
														{note.attachment_count}
													</span>
												{/if}
												{#if note.drawing_count}
													<span class="flex items-center gap-1">
														<Image size={12} />
														{note.drawing_count}
													</span>
												{/if}
											</div>
										{/if}
									</div>
									<span class="hidden text-xs text-base-content/55 sm:block">
										{note.modified_at ? new Date(note.modified_at).toLocaleDateString() : ''}
									</span>
								</a>
								{@render noteActions(note, 'list')}
							</div>
						{/each}
					</div>
				{/if}

				<div
					class="flex items-center justify-between border-t border-base-200 px-4 py-3 text-sm text-base-content/60"
				>
					<span>{filteredNotes.length} {itemPlural}</span>
					<label class="flex items-center gap-2">
						<span>Items per page</span>
						<select class="select-bordered select w-20 select-sm" bind:value={itemsPerPage}>
							<option value={20}>20</option>
							<option value={50}>50</option>
						</select>
					</label>
				</div>
			</div>
		{/if}
	</div>
</ModulePageShell>

{#snippet noteActions(note: any, position: 'list' | 'grid')}
	<div class="dropdown dropdown-end {position === 'grid' ? 'absolute top-3 right-3' : ''}">
		<button tabindex="0" class="btn btn-ghost btn-sm" aria-label="More options">
			<MoreHorizontal size={16} />
		</button>
		<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
		<ul
			tabindex="0"
			class="dropdown-content menu z-10 w-48 menu-sm rounded-box bg-base-200 p-1 shadow"
		>
			<li>
				<button onclick={() => handleShowAttachments(note)}>
					<Paperclip size={14} />
					Show attachments
					{#if note.attachment_count}
						<span class="badge badge-sm">{note.attachment_count}</span>
					{/if}
				</button>
			</li>
			<li>
				<button onclick={() => openRenameModal(note)}>
					<Pencil size={14} />
					Rename note
				</button>
			</li>
			<li>
				<button onclick={() => openMoveModal(note)}>
					<FolderInput size={14} />
					Move to folder
				</button>
			</li>
			<li>
				<button onclick={() => handleDuplicate(note)}>
					<Copy size={14} />
					Duplicate note
				</button>
			</li>
			<div class="divider my-0"></div>
			<li>
				<button onclick={() => openDeleteModal(note)} class="text-error">
					<Trash2 size={14} />
					Delete note
				</button>
			</li>
		</ul>
	</div>
{/snippet}

<PromptModal
	open={showRenameModal}
	title="Rename note"
	message="New title"
	defaultValue={activeNote
		? (activeNote.metadata?.title || activeNote.name || '').replace(/\.md$/i, '')
		: ''}
	confirmLabel="Rename"
	error={renameError}
	isLoading={isRenaming}
	onConfirm={handleRenameConfirm}
	onCancel={() => {
		showRenameModal = false;
		renameError = '';
	}}
/>

<MoveModal
	open={showMoveModal}
	loading={isMoving}
	itemName={activeNote
		? (activeNote.metadata?.title || activeNote.name || '').replace(/\.md$/i, '')
		: ''}
	itemType="file"
	currentFolderId={activeNote?.parent_folder_id ?? null}
	itemId={activeNote?.id ?? null}
	onClose={() => (showMoveModal = false)}
	onConfirm={handleMoveConfirm}
/>

<DeleteConfirmation
	open={showDeleteModal}
	loading={isDeleting}
	itemName={activeNote
		? (activeNote.metadata?.title || activeNote.name || '').replace(/\.md$/i, '')
		: ''}
	itemType="file"
	onClose={() => (showDeleteModal = false)}
	onConfirm={handleDeleteConfirm}
/>
