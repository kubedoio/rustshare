<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ApplicationPageSkeleton from '$lib/components/common/ApplicationPageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ApplicationPageShell from '$lib/components/layout/ApplicationPageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
	import {
		CalendarDays,
		Plus,
		Clock,
		Folder,
		Users,
		Search,
		List,
		Grid3X3,
		ArrowUpDown,
		MoreHorizontal,
		Paperclip,
		Pencil,
		FolderInput,
		Copy,
		Trash2
	} from 'lucide-svelte';

	import { meetingsApi } from '$lib/api/meetings';
	import { activityStore } from '$lib/stores/activity';
	import { resolveApplicationFolderId } from '$lib/applications/applicationPages';
	import type { ApplicationDefinition } from '$lib/applications/registry';
	import { toastStore } from '$lib/stores/toast';

	let { module }: { module: ApplicationDefinition } = $props();

	const meetingsQuery = createQuery({
		queryKey: ['meetings'],
		queryFn: () => meetingsApi.list()
	});

	let meetings = $derived($meetingsQuery.data ?? []);
	let searchTerm = $state('');
	let statusFilter = $state<'all' | 'recent'>('all');
	let sortDirection = $state<'desc' | 'asc'>('desc');
	let viewMode = $state<'list' | 'grid'>('list');
	let itemsPerPage = $state(20);

	$effect(() => {
		viewMode = module.ui.page.layout === 'gallery-grid' ? 'grid' : 'list';
	});
	let filteredMeetings = $derived(
		meetings
			.filter((meeting) =>
				(meeting.metadata?.title || meeting.name || '')
					.toLowerCase()
					.includes(searchTerm.trim().toLowerCase())
			)
			.filter((meeting) => {
				if (statusFilter === 'all') return true;
				const timestamp = new Date(
					meeting.modified_at ?? meeting.metadata?.updated_at ?? 0
				).getTime();
				return timestamp >= Date.now() - 30 * 24 * 60 * 60 * 1000;
			})
			.toSorted((a, b) => {
				const aTime = new Date(a.modified_at ?? a.metadata?.updated_at ?? 0).getTime();
				const bTime = new Date(b.modified_at ?? b.metadata?.updated_at ?? 0).getTime();
				return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
			})
	);
	let visibleMeetings = $derived(filteredMeetings.slice(0, itemsPerPage));

	let createError = $state('');
	let isCreating = $state(false);
	let autoCreateTriggered = $state(false);
	let showPromptModal = $state(false);

	let activeItem = $state<any>(null);
	let showRenameModal = $state(false);
	let showMoveModal = $state(false);
	let showDeleteModal = $state(false);
	let renameError = $state('');
	let isRenaming = $state(false);
	let isMoving = $state(false);
	let isDeleting = $state(false);
	let isDuplicating = $state(false);

	$effect(() => {
		const action = $page.url.searchParams.get('action');
		if (action === 'new' && !autoCreateTriggered && !isCreating) {
			autoCreateTriggered = true;
			showPromptModal = true;
			createError = '';
		}
	});

	function handleNewMeeting() {
		showPromptModal = true;
		createError = '';
	}

	async function handleCreateMeetingConfirm(name: string) {
		if (isCreating) return;
		isCreating = true;
		createError = '';

		const trimmed = name.trim();
		if (!trimmed) {
			createError = 'Title is required';
			isCreating = false;
			return;
		}

		let title = trimmed;
		const existingNames = meetings.map((m) => m.name?.toLowerCase() ?? '');
		if (existingNames.includes(title.toLowerCase())) {
			let counter = 2;
			while (existingNames.includes(`${title} ${counter}`.toLowerCase())) {
				counter++;
			}
			title = `${title} ${counter}`;
		}

		const content = `# ${title}\n\n## Agenda\n- \n\n## Attendees\n- \n\n## Notes\n- \n\n## Decisions\n- \n\n## Action Items\n- [ ] \n`;

		try {
			const result = await meetingsApi.create({
				title,
				team: 'General',
				date: new Date().toISOString(),
				content
			});
			showPromptModal = false;
			activityStore.addActivity(
				'meeting_created',
				result.name || title || 'Untitled Meeting Note',
				{
					artifactId: result.id,
					applicationId: 'io.elembra.meetings'
				}
			);
			goto(`/apps/${module.key}/${result.id}`);
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to create meeting:', err);
			createError = err instanceof Error ? err.message : 'Failed to create meeting';
		} finally {
			isCreating = false;
		}
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveApplicationFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function itemTitle(item: any): string {
		return (item.metadata?.title || item.name || '').replace(/\.md$/i, '');
	}

	function handleShowAttachments(item: any) {
		goto(`/apps/${module.key}/${item.id}?attachments=open`);
	}

	function openRenameModal(item: any) {
		activeItem = item;
		showRenameModal = true;
		renameError = '';
	}

	function openMoveModal(item: any) {
		activeItem = item;
		showMoveModal = true;
	}

	function openDeleteModal(item: any) {
		activeItem = item;
		showDeleteModal = true;
	}

	async function handleRenameConfirm(newTitle: string) {
		if (isRenaming || !activeItem) return;
		const trimmed = newTitle.trim();
		if (!trimmed) {
			renameError = 'Title is required';
			return;
		}
		isRenaming = true;
		renameError = '';
		try {
			await meetingsApi.rename(activeItem.id, { title: trimmed });
			toastStore.show('Meeting note renamed', 'success');
			showRenameModal = false;
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to rename meeting:', err);
			renameError = err instanceof Error ? err.message : 'Failed to rename';
		} finally {
			isRenaming = false;
		}
	}

	async function handleMoveConfirm(payload: { targetFolderId: string | null }) {
		if (isMoving || !activeItem) return;
		isMoving = true;
		try {
			await meetingsApi.move(activeItem.id, { target_folder_id: payload.targetFolderId });
			toastStore.show('Meeting note moved', 'success');
			showMoveModal = false;
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to move meeting:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to move', 'error');
		} finally {
			isMoving = false;
		}
	}

	async function handleDuplicate(item: any) {
		if (isDuplicating) return;
		isDuplicating = true;
		try {
			const duplicated = await meetingsApi.duplicate(item.id);
			toastStore.show('Meeting note duplicated', 'success');
			goto(`/apps/${module.key}/${duplicated.id}`);
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to duplicate meeting:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to duplicate meeting', 'error');
		} finally {
			isDuplicating = false;
		}
	}

	async function handleDeleteConfirm() {
		if (isDeleting || !activeItem) return;
		isDeleting = true;
		try {
			await meetingsApi.delete(activeItem.id);
			toastStore.show('Meeting note deleted', 'success');
			showDeleteModal = false;
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to delete meeting:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to delete', 'error');
		} finally {
			isDeleting = false;
		}
	}

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No meeting notes yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New meeting note');
	let searchPlaceholder = $derived(module.ui.page.searchPlaceholder ?? 'Search meeting notes...');
	let sortLabel = $derived(sortDirection === 'desc' ? 'Modified' : 'Oldest first');
	let itemPlural = $derived(module.ui.page.itemPlural ?? 'meeting notes');
</script>

<ApplicationPageShell
	title="Meeting Notes"
	subtitle="Record simple meeting notes, decisions, and follow-up items."
>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleNewMeeting} disabled={isCreating}>
			<Plus size={14} />
			<span>New meeting note</span>
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
		{#if $meetingsQuery.isLoading}
			<ApplicationPageSkeleton />
		{:else if $meetingsQuery.isError}
			<ErrorState
				title="Failed to load meetings"
				message={$meetingsQuery.error?.message || 'Unknown error'}
				onRetry={() => $meetingsQuery.refetch()}
			/>
		{:else if meetings.length === 0}
			<EmptyState
				icon={'📅'}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleNewMeeting}
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
						aria-label="Filter meetings"
					>
						<option value="all">{module.ui.page.filterLabel ?? 'All notes'}</option>
						<option value="recent">Last 30 days</option>
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
						{#each visibleMeetings as meeting}
							<div
								class="relative rounded-xl border border-base-300/50 p-4 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
							>
								<a href={`/apps/${module.key}/${meeting.id}`} class="block">
									<div
										class="flex h-9 w-9 items-center justify-center rounded-full bg-brand-500/10 text-brand-500 mb-3"
									>
										<CalendarDays size={16} />
									</div>
									<div class="flex min-w-0 flex-1 flex-col">
										<span class="truncate text-sm font-medium text-base-content">
											{itemTitle(meeting)}
										</span>
										<div class="flex items-center gap-2 text-xs text-base-content/55">
											{#if meeting.metadata?.date}
												<span>{new Date(meeting.metadata.date).toLocaleDateString()}</span>
											{/if}
											{#if meeting.metadata?.attendees?.length > 0}
												<span>•</span>
												<span class="inline-flex items-center gap-1">
													<Users size={12} />
													{meeting.metadata.attendees.length} attendees
												</span>
											{/if}
										</div>
									</div>
									<span class="mt-3 block text-xs text-base-content/55">
										{meeting.modified_at ? new Date(meeting.modified_at).toLocaleDateString() : ''}
									</span>
								</a>
								{@render itemActions(meeting, 'grid')}
							</div>
						{/each}
					</div>
				{:else}
					<div class="divide-y divide-base-200">
						{#each visibleMeetings as meeting}
							<div class="flex items-center gap-4 px-4 py-3 transition-colors hover:bg-base-200/40">
								<a
									href={`/apps/${module.key}/${meeting.id}`}
									class="flex min-w-0 flex-1 items-center gap-4"
								>
									<div
										class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-brand-500/10 text-brand-500"
									>
										<CalendarDays size={16} />
									</div>
									<div class="flex min-w-0 flex-1 flex-col">
										<span class="truncate text-sm font-medium text-base-content">
											{itemTitle(meeting)}
										</span>
										<div class="flex items-center gap-2 text-xs text-base-content/55">
											{#if meeting.metadata?.date}
												<span>{new Date(meeting.metadata.date).toLocaleDateString()}</span>
											{/if}
											{#if meeting.metadata?.attendees?.length > 0}
												<span>•</span>
												<span class="inline-flex items-center gap-1">
													<Users size={12} />
													{meeting.metadata.attendees.length} attendees
												</span>
											{/if}
										</div>
									</div>
									<span class="hidden sm:block text-xs text-base-content/55">
										{meeting.modified_at ? new Date(meeting.modified_at).toLocaleDateString() : ''}
									</span>
								</a>
								{@render itemActions(meeting, 'list')}
							</div>
						{/each}
					</div>
				{/if}

				<div
					class="flex items-center justify-between border-t border-base-200 px-4 py-3 text-sm text-base-content/60"
				>
					<span>{filteredMeetings.length} {itemPlural}</span>
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
</ApplicationPageShell>

{#snippet itemActions(item: any, position: 'list' | 'grid')}
	<div class="dropdown dropdown-end {position === 'grid' ? 'absolute top-3 right-3' : ''}">
		<button tabindex="0" class="btn btn-ghost btn-sm" aria-label="More options">
			<MoreHorizontal size={16} />
		</button>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<ul
			tabindex="0"
			class="dropdown-content menu z-10 w-48 menu-sm rounded-box bg-base-200 p-1 shadow"
		>
			<li>
				<button onclick={() => handleShowAttachments(item)}>
					<Paperclip size={14} />
					Show attachments
				</button>
			</li>
			<li>
				<button onclick={() => openRenameModal(item)}>
					<Pencil size={14} />
					Rename
				</button>
			</li>
			<li>
				<button onclick={() => openMoveModal(item)}>
					<FolderInput size={14} />
					Move to folder
				</button>
			</li>
			<li>
				<button onclick={() => handleDuplicate(item)}>
					<Copy size={14} />
					Duplicate
				</button>
			</li>
			<div class="divider my-0"></div>
			<li>
				<button onclick={() => openDeleteModal(item)} class="text-error">
					<Trash2 size={14} />
					Delete
				</button>
			</li>
		</ul>
	</div>
{/snippet}

<PromptModal
	open={showPromptModal}
	title="New meeting note"
	message="Meeting title"
	placeholder="e.g. Weekly Standup"
	confirmLabel="Create"
	error={createError}
	isLoading={isCreating}
	onConfirm={handleCreateMeetingConfirm}
	onCancel={() => {
		showPromptModal = false;
		createError = '';
	}}
/>

<PromptModal
	open={showRenameModal}
	title="Rename meeting note"
	message="New title"
	defaultValue={activeItem ? itemTitle(activeItem) : ''}
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
	itemName={activeItem ? itemTitle(activeItem) : ''}
	itemType="folder"
	currentFolderId={activeItem?.parent_folder_id ?? null}
	itemId={activeItem?.id ?? null}
	onClose={() => (showMoveModal = false)}
	onConfirm={handleMoveConfirm}
/>

<DeleteConfirmation
	open={showDeleteModal}
	loading={isDeleting}
	itemName={activeItem ? itemTitle(activeItem) : ''}
	itemType="folder"
	onClose={() => (showDeleteModal = false)}
	onConfirm={handleDeleteConfirm}
/>
