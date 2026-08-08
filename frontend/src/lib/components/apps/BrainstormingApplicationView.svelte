<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { listBrainstormBoards, createBrainstormBoard } from '$lib/api/brainstorming';
	import { activityStore } from '$lib/stores/activity';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ApplicationPageSkeleton from '$lib/components/common/ApplicationPageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ApplicationPageShell from '$lib/components/layout/ApplicationPageShell.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { resolveApplicationFolderId } from '$lib/applications/applicationPages';
	import type { ApplicationDefinition } from '$lib/applications/registry';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import { toastStore } from '$lib/stores/toast';
	import {
		Lightbulb,
		Plus,
		Clock,
		ImageOff,
		Folder,
		Search,
		List,
		Grid3X3,
		Share2
	} from 'lucide-svelte';
	import { formatDistanceToNow } from '$lib/utils/format';

	interface Props {
		module: ApplicationDefinition;
	}

	let { module }: Props = $props();

	const queryClient = useQueryClient();
	let showCreateModal = $state(false);
	let newBoardTitle = $state('');
	let createError = $state('');
	let brokenPreviews = $state(new Set<string>());
	let searchTerm = $state('');
	let viewMode = $state<'list' | 'grid'>('grid');
	let showShareModal = $state(false);
	let shareBoardId = $state('');
	let shareBoardTitle = $state('');

	$effect(() => {
		viewMode =
			module.ui.page.layout === 'list-grid' || module.ui.page.layout === 'file-list'
				? 'list'
				: 'grid';
	});

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No idea boards yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No idea boards yet. Create a simple visual board to capture sketches, flows, or early thinking.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New idea board');
	let searchPlaceholder = $derived(module.ui.page.searchPlaceholder ?? 'Search boards...');
	let filterLabel = $derived(module.ui.page.filterLabel ?? 'All boards');
	let sortLabel = $derived(module.ui.page.sortLabel ?? 'Last updated');

	const boardsQuery = createQuery({
		queryKey: ['brainstorm-boards'],
		queryFn: () => listBrainstormBoards()
	});
	let boards = $derived(
		($boardsQuery.data ?? []).filter((board) =>
			board.title.toLowerCase().includes(searchTerm.trim().toLowerCase())
		)
	);

	const createBoardMutation = createMutation({
		mutationFn: ({ title, templateKey }: { title: string; templateKey: string }) =>
			createBrainstormBoard(title, templateKey),
		onSuccess: (data) => {
			queryClient.invalidateQueries({ queryKey: ['brainstorm-boards'] });
			showCreateModal = false;
			newBoardTitle = '';
			createError = '';
			activityStore.addActivity('brainstorm_created', data.title || 'Untitled Idea Board', {
				artifactId: data.id,
				applicationId: 'brainstorming'
			});
			goto(`/apps/brainstorming/${data.id}`);
		},
		onError: (err: Error) => {
			createError = err.message || 'Failed to create board';
		}
	});

	function handleCreateBoard() {
		showCreateModal = true;
	}

	function handleSubmit() {
		const title = newBoardTitle.trim();
		if (!title) {
			createError = 'Title is required';
			return;
		}

		createBoardMutation.mutate({ title, templateKey: 'template_blank_brainstorm' });
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveApplicationFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function handleShareBoard(board: { id: string; title: string }, event: MouseEvent) {
		event.preventDefault();
		event.stopPropagation();
		shareBoardId = board.id;
		shareBoardTitle = board.title;
		showShareModal = true;
	}

	function getPreviewUrl(board: { preview_file_id: string | null }): string | null {
		if (!board.preview_file_id) return null;
		return `/api/v1/files/${board.preview_file_id}/content`;
	}
</script>

<ApplicationPageShell
	title="Brainstorming"
	subtitle="Capture sketches, flows, and early ideas as visual workspace boards."
>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
			<Plus size={14} />
			<span>New idea board</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>
	<div class="flex flex-col gap-4">
		{#if $boardsQuery.isLoading}
			<ApplicationPageSkeleton />
		{:else if $boardsQuery.isError}
			<ErrorState
				title="Failed to load boards"
				message={$boardsQuery.error?.message || 'Unknown error'}
				onRetry={() => $boardsQuery.refetch()}
			/>
		{:else if ($boardsQuery.data ?? []).length === 0}
			<EmptyState
				icon={'💡'}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateBoard}
			/>
		{:else}
			<div class="flex flex-col gap-4">
				<div class="flex flex-col gap-3 lg:flex-row lg:items-center">
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
					<button class="btn justify-between btn-sm btn-outline lg:w-36" disabled
						>{filterLabel}</button
					>
					<div class="ml-auto flex items-center gap-2">
						<span class="text-xs font-medium text-base-content/60">{sortLabel}</span>
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
				<div
					class={viewMode === 'grid'
						? 'grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3'
						: 'flex flex-col gap-3'}
				>
					{#each boards as board}
						<a
							href={`/apps/brainstorming/${board.id}`}
							class={viewMode === 'grid'
								? 'group flex flex-col gap-3 rounded-xl border border-base-300/40 p-3 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm'
								: 'group flex items-center gap-4 rounded-xl border border-base-300/40 p-3 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm'}
						>
							<div
								class={viewMode === 'grid'
									? 'relative aspect-[4/3] overflow-hidden rounded-lg bg-base-200'
									: 'flex h-12 w-12 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-base-200'}
							>
								{#if getPreviewUrl(board) && !brokenPreviews.has(board.id)}
									<img
										src={getPreviewUrl(board)!}
										alt={board.title}
										class="h-full w-full object-cover transition-transform group-hover:scale-105"
										loading="lazy"
										onerror={() => brokenPreviews.add(board.id)}
									/>
								{:else}
									<div
										class="flex h-full w-full flex-col items-center justify-center gap-2 text-base-content/30"
									>
										<Lightbulb size={32} />
										<span class="text-xs">Idea board</span>
									</div>
								{/if}
							</div>
							<div class="flex flex-col gap-1 px-1">
								<span class="text-sm font-medium text-base-content">{board.title}</span>
								<div class="flex items-center justify-between">
									<span class="flex items-center gap-1 text-xs text-base-content/40">
										<Clock size={12} />
										{board.updated_at
											? formatDistanceToNow(new Date(board.updated_at), { addSuffix: true })
											: ''}
									</span>
									<button
										class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs font-medium text-brand-600 transition-colors hover:bg-brand-500/10"
										onclick={(e) => handleShareBoard(board, e)}
										type="button"
									>
										<Share2 size={12} />
										Share
									</button>
								</div>
							</div>
						</a>
					{/each}
					{#if viewMode === 'grid'}
						<button
							class="flex min-h-44 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-base-300/70 p-4 text-center text-base-content/45 transition-colors hover:border-brand-500/40 hover:text-brand-500"
							onclick={handleCreateBoard}
						>
							<Lightbulb size={34} />
							<span class="text-sm font-semibold">No idea board yet</span>
							<span class="max-w-40 text-xs">Create a board to start visualizing your ideas.</span>
						</button>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</ApplicationPageShell>

<!-- Create Board Modal -->
<ModalBase
	open={showCreateModal}
	title="New idea board"
	onClose={() => {
		showCreateModal = false;
		newBoardTitle = '';
		createError = '';
	}}
>
	<div class="flex flex-col gap-4">
		<div>
			<label
				for="board-title"
				class="label-text mb-1 block text-xs font-semibold text-base-content/70"
			>
				Board name
			</label>
			<input
				id="board-title"
				type="text"
				class="input-bordered input w-full"
				placeholder="e.g. Product launch ideas"
				bind:value={newBoardTitle}
				onkeydown={(e) => {
					if (e.key === 'Enter') handleSubmit();
					if (e.key === 'Escape') {
						showCreateModal = false;
						newBoardTitle = '';
						createError = '';
					}
				}}
			/>
		</div>

		{#if createError}
			<p class="text-sm text-error">{createError}</p>
		{/if}

		<div class="flex flex-col items-center gap-2 py-4 text-center text-base-content/55">
			<Lightbulb size={34} />
			<p class="text-sm">You'll start with a blank board.</p>
		</div>

		<div class="flex justify-end gap-2 pt-2">
			<button
				class="btn btn-ghost btn-sm"
				onclick={() => {
					showCreateModal = false;
					newBoardTitle = '';
					createError = '';
				}}
			>
				Cancel
			</button>
			<button
				class="btn btn-sm btn-primary"
				onclick={handleSubmit}
				disabled={$createBoardMutation.isPending}
			>
				{$createBoardMutation.isPending ? 'Creating...' : 'Create board'}
			</button>
		</div>
	</div>
</ModalBase>

<!-- Share Modal -->
<ShareModal
	open={showShareModal}
	resourceId={shareBoardId}
	resourceName={shareBoardTitle}
	resourceType="folder"
	onClose={() => {
		showShareModal = false;
		shareBoardId = '';
		shareBoardTitle = '';
	}}
	onNotification={(payload) => toastStore.show(payload.message, payload.type)}
/>
