<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { listBrainstormBoards, createBrainstormBoard } from '$lib/api/brainstorming';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { PenTool, Plus, Clock, ImageOff, Folder } from 'lucide-svelte';
	import { formatDistanceToNow } from 'date-fns';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const isList = $derived(
		module.ui.page.layout === 'list-grid' || module.ui.page.layout === 'file-list'
	);

	const queryClient = useQueryClient();
	let showCreateModal = $state(false);
	let newBoardTitle = $state('');
	let selectedTemplate = $state('template_blank_brainstorm');
	let createError = $state('');

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No brainstorming boards yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ?? 'Create your first visual decision board.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New Board');

	const boardsQuery = createQuery({
		queryKey: ['brainstorm-boards'],
		queryFn: () => listBrainstormBoards()
	});

	const createBoardMutation = createMutation({
		mutationFn: ({ title, templateKey }: { title: string; templateKey: string }) =>
			createBrainstormBoard(title, templateKey),
		onSuccess: (data) => {
			queryClient.invalidateQueries({ queryKey: ['brainstorm-boards'] });
			showCreateModal = false;
			newBoardTitle = '';
			createError = '';
			goto(`/modules/brainstorming/${data.id}`);
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
		createBoardMutation.mutate({ title, templateKey: selectedTemplate });
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function getPreviewUrl(board: { preview_file_id: string | null }): string | null {
		if (!board.preview_file_id) return null;
		return `/api/v1/files/${board.preview_file_id}/content`;
	}

	const templates = [
		{ key: 'template_blank_brainstorm', label: 'Blank Board' },
		{ key: 'template_decision_making_brainstorm', label: 'Decision Making & Brainstorming' },
		{ key: 'template_meeting_whiteboard', label: 'Meeting Whiteboard' }
	];
</script>

<ModulePageShell title="Brainstorming" subtitle={module.description}>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
			<Plus size={14} />
			<span>New Board</span>
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
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if ($boardsQuery.data ?? []).length === 0}
			<EmptyState
				icon={PenTool}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateBoard}
			/>
		{:else if isList}
			<div class="flex flex-col gap-3">
				{#each $boardsQuery.data ?? [] as board}
					<a
						href={`/modules/brainstorming/${board.id}`}
						class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
					>
						<div
							class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
						>
							<PenTool size={18} />
						</div>
						<div class="flex min-w-0 flex-col gap-1">
							<span class="truncate text-sm font-medium text-base-content">{board.title}</span>
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{board.updated_at
									? formatDistanceToNow(new Date(board.updated_at), { addSuffix: true })
									: ''}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each $boardsQuery.data ?? [] as board}
					<a
						href={`/modules/brainstorming/${board.id}`}
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-3 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div class="relative aspect-[4/3] overflow-hidden rounded-lg bg-base-200">
							{#if getPreviewUrl(board)}
								<img
									src={getPreviewUrl(board)!}
									alt={board.title}
									class="h-full w-full object-cover transition-transform group-hover:scale-105"
									loading="lazy"
								/>
							{:else}
								<div
									class="flex h-full w-full flex-col items-center justify-center gap-2 text-base-content/30"
								>
									<ImageOff size={32} />
									<span class="text-xs">No preview</span>
								</div>
							{/if}
						</div>
						<div class="flex flex-col gap-1 px-1">
							<span class="text-sm font-medium text-base-content">{board.title}</span>
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{board.updated_at
									? formatDistanceToNow(new Date(board.updated_at), { addSuffix: true })
									: ''}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>

<!-- Create Board Modal -->
<ModalBase
	open={showCreateModal}
	title="New Brainstorming Board"
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
				Board Title
			</label>
			<input
				id="board-title"
				type="text"
				class="input-bordered input w-full"
				placeholder="e.g., Q3 Product Roadmap"
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

		<div>
			<span class="label-text mb-1 block text-xs font-semibold text-base-content/70">Template</span>
			<div class="flex flex-col gap-2">
				{#each templates as tmpl}
					<label
						class="flex cursor-pointer items-center gap-3 rounded-lg border border-base-300/40 p-3 transition-colors hover:bg-base-200/30"
					>
						<input
							type="radio"
							name="template"
							value={tmpl.key}
							bind:group={selectedTemplate}
							class="radio radio-sm radio-primary"
						/>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{tmpl.label}</span>
						</div>
					</label>
				{/each}
			</div>
		</div>

		{#if createError}
			<p class="text-sm text-error">{createError}</p>
		{/if}

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
				{$createBoardMutation.isPending ? 'Creating...' : 'Create Board'}
			</button>
		</div>
	</div>
</ModalBase>
