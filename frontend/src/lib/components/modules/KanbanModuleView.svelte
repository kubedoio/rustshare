<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import {
		listKanbanBoards,
		getKanbanBoard,
		createKanbanBoard,
		createKanbanCard,
		updateKanbanCard,
		moveKanbanCard,
		archiveKanbanCard,
		deleteKanbanCard
	} from '$lib/api/kanban';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import CreateKanbanBoardModal from '$lib/components/modals/CreateKanbanBoardModal.svelte';
	import type { KanbanBoard, KanbanCard, KanbanColumn } from '$lib/api/types';
	import { Folder, Plus, GripVertical, Archive, Trash2, X, ChevronRight } from 'lucide-svelte';

	import RichMarkdownEditor from '../../editor/components/RichMarkdownEditor.svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const queryClient = useQueryClient();

	let selectedBoardId = $state('');
	let showCreateBoardModal = $state(false);
	let dragCardId = $state<string | null>(null);
	let dragSourceColumnId = $state<string | null>(null);
	let dragSourceOrder = $state(0);
	let errorMessage = $state('');
	let showCreateCardColumnId = $state<string | null>(null);
	let newCardTitle = $state('');
	let editingCard = $state<KanbanCard | null>(null);
	let editCardTitle = $state('');
	let editCardContent = $state('');
	let draggingOverColumnId = $state<string | null>(null);
	let columnRefs = $state<Record<string, HTMLDivElement | null>>({});

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No boards yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ?? 'Create your first file-backed board.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New Board');

	// -------------------------------------------------------------------------
	// Queries
	// -------------------------------------------------------------------------

	const boardsQuery = createQuery({
		queryKey: ['kanban-boards'],
		queryFn: () => listKanbanBoards()
	});

	let boardQuery = $derived(
		createQuery({
			queryKey: ['kanban-board', selectedBoardId],
			queryFn: () => getKanbanBoard(selectedBoardId),
			enabled: !!selectedBoardId
		})
	);

	$effect(() => {
		const boards = $boardsQuery.data ?? [];
		if (!selectedBoardId && boards.length > 0) {
			selectedBoardId = boards[0].id;
		}
	});

	$effect(() => {
		const boards = $boardsQuery.data ?? [];
		if (selectedBoardId && boards.length > 0 && !boards.some((b) => b.id === selectedBoardId)) {
			selectedBoardId = boards[0].id;
		}
	});

	const selectedBoard = $derived($boardQuery.data);

	// -------------------------------------------------------------------------
	// Mutations
	// -------------------------------------------------------------------------

	const createCardMutation = createMutation({
		mutationFn: ({
			boardId,
			input
		}: {
			boardId: string;
			input: Parameters<typeof createKanbanCard>[1];
		}) => createKanbanCard(boardId, input),
		onSuccess: () => {
			boardQuery.refetch();
			showCreateCardColumnId = null;
			newCardTitle = '';
		}
	});

	const moveCardMutation = createMutation({
		mutationFn: ({
			cardId,
			columnId,
			order
		}: {
			cardId: string;
			columnId: string;
			order: number;
		}) => moveKanbanCard(cardId, columnId, order),
		onSuccess: () => {
			errorMessage = '';
			boardQuery.refetch();
		},
		onError: () => {
			errorMessage = 'Card move failed. The board was restored to its previous state.';
			boardQuery.refetch();
		}
	});

	const updateCardMutation = createMutation({
		mutationFn: ({
			cardId,
			input
		}: {
			cardId: string;
			input: Parameters<typeof updateKanbanCard>[1];
		}) => updateKanbanCard(cardId, input),
		onSuccess: () => {
			boardQuery.refetch();
			editingCard = null;
		}
	});

	const archiveCardMutation = createMutation({
		mutationFn: archiveKanbanCard,
		onSuccess: () => {
			boardQuery.refetch();
			editingCard = null;
		}
	});

	const deleteCardMutation = createMutation({
		mutationFn: deleteKanbanCard,
		onSuccess: () => {
			boardQuery.refetch();
			editingCard = null;
		}
	});

	// -------------------------------------------------------------------------
	// Board creation
	// -------------------------------------------------------------------------

	function handleCreateBoard() {
		showCreateBoardModal = true;
	}

	function handleBoardCreated(boardId: string) {
		selectedBoardId = boardId;
		boardsQuery.refetch();
	}

	// -------------------------------------------------------------------------
	// Drag and drop
	// -------------------------------------------------------------------------

	function handleDragStart(e: DragEvent, card: KanbanCard, columnId: string) {
		dragCardId = card.id;
		dragSourceColumnId = columnId;
		dragSourceOrder = card.order;
		if (e.dataTransfer) {
			e.dataTransfer.effectAllowed = 'move';
			e.dataTransfer.setData('text/plain', card.id);
		}
	}

	function handleDragEnd() {
		dragCardId = null;
		dragSourceColumnId = null;
		draggingOverColumnId = null;
	}

	function handleDragOver(e: DragEvent, columnId: string) {
		e.preventDefault();
		if (e.dataTransfer) {
			e.dataTransfer.dropEffect = 'move';
		}
		draggingOverColumnId = columnId;
	}

	function handleDragLeave(columnId: string) {
		if (draggingOverColumnId === columnId) {
			draggingOverColumnId = null;
		}
	}

	function getDropIndex(container: HTMLDivElement, clientY: number): number {
		const cards = Array.from(container.querySelectorAll<HTMLElement>('[data-card-id]')).filter(
			(el) => el.dataset.cardId !== dragCardId
		);
		for (let i = 0; i < cards.length; i++) {
			const rect = cards[i].getBoundingClientRect();
			const midY = rect.top + rect.height / 2;
			if (clientY < midY) return i;
		}
		return cards.length;
	}

	function computeTargetOrder(
		column: KanbanColumn,
		insertIndex: number,
		movingCardId: string
	): number {
		const cards = column.cards
			.filter((c) => c.id !== movingCardId)
			.sort((a, b) => a.order - b.order);

		if (cards.length === 0) return 1000;
		if (insertIndex <= 0) return Math.round(cards[0].order / 2);
		if (insertIndex >= cards.length) return cards[cards.length - 1].order + 1000;
		return Math.round((cards[insertIndex - 1].order + cards[insertIndex].order) / 2);
	}

	function handleDrop(e: DragEvent, targetColumnId: string) {
		e.preventDefault();
		draggingOverColumnId = null;

		if (!dragCardId || !dragSourceColumnId) return;
		if (dragCardId === '') return;

		const board = selectedBoard;
		if (!board) return;

		const targetColumn = board.columns.find((c) => c.id === targetColumnId);
		if (!targetColumn) return;

		const container = columnRefs[targetColumnId];
		if (!container) return;

		const insertIndex = getDropIndex(container, e.clientY);
		const targetOrder = computeTargetOrder(targetColumn, insertIndex, dragCardId);

		// If same column and same order, skip
		if (dragSourceColumnId === targetColumnId && Math.abs(dragSourceOrder - targetOrder) < 1) {
			handleDragEnd();
			return;
		}

		// Optimistic update
		const queryKey = ['kanban-board', selectedBoardId];
		const oldBoard = queryClient.getQueryData<KanbanBoard>(queryKey);

		if (oldBoard) {
			const newBoard: KanbanBoard = {
				...oldBoard,
				columns: oldBoard.columns.map((col) => {
					let newCards = [...col.cards];
					let updated = false;

					if (col.id === dragSourceColumnId) {
						newCards = newCards.filter((c) => c.id !== dragCardId);
						updated = true;
					}

					if (col.id === targetColumnId) {
						const card = oldBoard.columns
							.find((c) => c.id === dragSourceColumnId)
							?.cards.find((c) => c.id === dragCardId);
						if (card) {
							newCards.push({ ...card, column_id: targetColumnId, order: targetOrder });
							newCards.sort((a, b) => a.order - b.order);
							updated = true;
						}
					}

					return updated ? { ...col, cards: newCards } : col;
				})
			};
			queryClient.setQueryData(queryKey, newBoard);
		}

		moveCardMutation.mutate({
			cardId: dragCardId,
			columnId: targetColumnId,
			order: targetOrder
		});

		handleDragEnd();
	}

	// -------------------------------------------------------------------------
	// Card creation
	// -------------------------------------------------------------------------

	function handleCreateCard(columnId: string) {
		if (!newCardTitle.trim() || !selectedBoardId) return;
		createCardMutation.mutate({
			boardId: selectedBoardId,
			input: {
				title: newCardTitle.trim(),
				column_id: columnId
			}
		});
	}

	// -------------------------------------------------------------------------
	// Card editing
	// -------------------------------------------------------------------------

	function openCardEdit(card: KanbanCard) {
		editingCard = card;
		editCardTitle = card.title;
		editCardContent = card.content;
	}

	function saveCardEdit() {
		if (!editingCard) return;
		updateCardMutation.mutate({
			cardId: editingCard.id,
			input: {
				title: editCardTitle.trim(),
				content: editCardContent
			}
		});
	}

	function handleArchiveCard() {
		if (!editingCard) return;
		if (confirm('Archive this card?')) {
			archiveCardMutation.mutate(editingCard.id);
		}
	}

	function handleDeleteCard() {
		if (!editingCard) return;
		if (confirm('Delete this card permanently?')) {
			deleteCardMutation.mutate(editingCard.id);
		}
	}

	function formatColumnName(value: string): string {
		return value.replace(/^\d+-/, '').replace(/-/g, ' ');
	}
</script>

<div class="flex flex-col gap-6">
	{#if $boardsQuery.isLoading}
		<div class="flex h-48 items-center justify-center">
			<div class="loading loading-md loading-spinner text-brand-500"></div>
		</div>
	{:else if ($boardsQuery.data ?? []).length === 0}
		<EmptyState
			icon={Folder}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	{:else}
		<!-- Header -->
		<div class="flex items-center justify-between gap-4">
			<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Boards</h2>
			<button class="btn btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>

		<!-- Board selector -->
		<div class="flex flex-wrap gap-2">
			{#each $boardsQuery.data ?? [] as board}
				<button
					type="button"
					class={`rounded-full border px-3 py-1.5 text-xs font-semibold transition-colors ${
						board.id === selectedBoardId
							? 'border-brand-500 bg-brand-500 text-white'
							: 'border-base-300/60 bg-base-100 text-base-content/70 hover:border-brand-500/40'
					}`}
					onclick={() => {
						selectedBoardId = board.id;
					}}
				>
					{board.title}
				</button>
			{/each}
		</div>

		<!-- Selected board -->
		{#if selectedBoard}
			<div class="flex items-center justify-between">
				<div>
					<h3 class="text-lg font-semibold text-base-content">{selectedBoard.title}</h3>
					<p class="text-sm text-base-content/55">
						{selectedBoard.columns.length} columns
					</p>
				</div>
			</div>

			{#if errorMessage}
				<div class="rounded-lg border border-red-300 bg-red-50 px-4 py-2 text-sm text-red-700">
					{errorMessage}
				</div>
			{/if}

			{#if $boardQuery.isLoading}
				<div
					class="flex h-48 items-center justify-center rounded-3xl border border-base-300/40 bg-base-100"
				>
					<div class="loading loading-md loading-spinner text-brand-500"></div>
				</div>
			{:else}
				<div class="kanban-board-surface">
					{#each selectedBoard.columns as column}
						<section
							class="kanban-column"
							class:kanban-column-dragover={draggingOverColumnId === column.id}
							ondragover={(e) => handleDragOver(e, column.id)}
							ondragleave={() => handleDragLeave(column.id)}
							ondrop={(e) => handleDrop(e, column.id)}
							aria-label="{column.title} column"
						>
							<header class="kanban-column-header">
								<h4>{formatColumnName(column.title)}</h4>
								<span>{column.cards.length}</span>
							</header>

							<div class="kanban-card-list" bind:this={columnRefs[column.id]}>
								{#if column.cards.length === 0}
									<div class="kanban-empty-column">No cards.</div>
								{/if}

								{#each column.cards as card (card.id)}
									<div
										class="kanban-card"
										draggable="true"
										data-card-id={card.id}
										role="button"
										tabindex="0"
										ondragstart={(e) => handleDragStart(e, card, column.id)}
										ondragend={handleDragEnd}
										onclick={() => openCardEdit(card)}
										onkeydown={(e) => {
											if (e.key === 'Enter' || e.key === ' ') openCardEdit(card);
										}}
									>
										<div class="kanban-card-title-row">
											<GripVertical size={14} class="text-base-content/30" />
											<strong>{card.title}</strong>
										</div>
										{#if card.priority !== 'normal'}
											<span
												class="text-[10px] font-semibold tracking-wider text-brand-500 uppercase"
											>
												{card.priority}
											</span>
										{/if}
									</div>
								{/each}
							</div>

							<div class="mt-2">
								{#if showCreateCardColumnId === column.id}
									<div
										class="flex flex-col gap-2 rounded-xl border border-base-300/60 bg-base-100 p-2"
									>
										<input
											type="text"
											placeholder="Card title"
											class="input-bordered input input-sm w-full"
											bind:value={newCardTitle}
											onkeydown={(e) => {
												if (e.key === 'Enter') handleCreateCard(column.id);
												if (e.key === 'Escape') {
													showCreateCardColumnId = null;
													newCardTitle = '';
												}
											}}
										/>
										<div class="flex gap-2">
											<button
												class="btn flex-1 btn-xs btn-primary"
												onclick={() => handleCreateCard(column.id)}
											>
												Add
											</button>
											<button
												class="btn btn-ghost btn-xs"
												onclick={() => {
													showCreateCardColumnId = null;
													newCardTitle = '';
												}}
											>
												Cancel
											</button>
										</div>
									</div>
								{:else}
									<button
										class="btn w-full text-base-content/60 btn-ghost btn-xs"
										onclick={() => {
											showCreateCardColumnId = column.id;
											newCardTitle = '';
										}}
									>
										<Plus size={12} />
										Add card
									</button>
								{/if}
							</div>
						</section>
					{/each}
				</div>
			{/if}
		{/if}
	{/if}
</div>

<!-- Card edit modal -->
<ModalBase
	open={editingCard !== null}
	title="Edit Card"
	onClose={() => {
		editingCard = null;
	}}
>
	{#if editingCard}
		<div class="flex flex-col gap-4">
			<div>
				<label
					for="edit-card-title"
					class="label-text mb-1 block text-xs font-semibold text-base-content/70">Title</label
				>
				<input
					id="edit-card-title"
					type="text"
					class="input-bordered input w-full"
					bind:value={editCardTitle}
				/>
			</div>
			<div>
				<label
					for="edit-card-content"
					class="label-text mb-1 block text-xs font-semibold text-base-content/70"
					>Content (Markdown)</label
				>
				<div
					class="flex min-h-[12rem] flex-col overflow-hidden rounded-xl border border-base-300 bg-base-100"
				>
					<RichMarkdownEditor
						content={editingCard.content}
						editable={true}
						bind:currentMarkdown={editCardContent}
						permissions={{
							canRead: true,
							canEdit: true,
							canUploadAttachments: true,
							canDeleteAttachments: true,
							canExport: false,
							canShare: true
						}}
					/>
				</div>
			</div>
			<div class="flex items-center justify-between gap-2 pt-2">
				<div class="flex gap-2">
					<button class="btn btn-sm btn-primary" onclick={saveCardEdit}>Save</button>
					<button
						class="btn btn-ghost btn-sm"
						onclick={() => {
							editingCard = null;
						}}
					>
						Cancel
					</button>
				</div>
				<div class="flex gap-2">
					<button class="btn text-amber-600 btn-ghost btn-sm" onclick={handleArchiveCard}>
						<Archive size={14} />
						Archive
					</button>
					<button class="btn text-red-600 btn-ghost btn-sm" onclick={handleDeleteCard}>
						<Trash2 size={14} />
						Delete
					</button>
				</div>
			</div>
		</div>
	{/if}
</ModalBase>

<CreateKanbanBoardModal
	open={showCreateBoardModal}
	onClose={() => (showCreateBoardModal = false)}
	onSuccess={handleBoardCreated}
	defaultTemplate={module.defaultTemplate}
/>

<style>
	.kanban-board-surface {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(16rem, 1fr);
		gap: 1rem;
		overflow-x: auto;
		padding-bottom: 0.75rem;
	}

	.kanban-column {
		display: flex;
		min-height: 26rem;
		flex-direction: column;
		gap: 0.75rem;
		border-radius: 1.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		background: color-mix(in oklab, var(--rs-surface-muted) 38%, white);
		padding: 1rem;
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}

	.kanban-column-dragover {
		border-color: color-mix(in oklab, var(--brand-500) 50%, transparent);
		background: color-mix(in oklab, var(--brand-500) 8%, var(--rs-surface-muted));
	}

	.kanban-column-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
	}

	.kanban-column-header h4 {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 800;
		color: var(--base-content);
	}

	.kanban-column-header span {
		display: inline-flex;
		min-width: 1.75rem;
		justify-content: center;
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.8);
		padding: 0.18rem 0.45rem;
		font-size: 0.72rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.kanban-card-list {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 0.6rem;
		min-height: 2rem;
	}

	.kanban-card {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		border-radius: 1rem;
		border: 1px solid rgba(133, 95, 44, 0.08);
		background: rgba(255, 255, 255, 0.85);
		padding: 0.75rem;
		cursor: pointer;
		text-align: left;
		box-shadow: 0 8px 18px rgb(72 42 17 / 0.05);
		transition:
			transform 160ms ease,
			border-color 160ms ease,
			box-shadow 160ms ease;
	}

	.kanban-card:hover {
		transform: translateY(-1px);
		border-color: color-mix(in oklab, var(--brand-500) 35%, transparent);
		box-shadow: 0 12px 24px rgb(72 42 17 / 0.08);
	}

	.kanban-card-title-row {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.kanban-card strong {
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.kanban-empty-column {
		border: 1px dashed color-mix(in oklab, var(--base-300) 60%, transparent);
		border-radius: 1rem;
		background: rgba(255, 255, 255, 0.45);
		padding: 1rem;
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}

	@media (max-width: 767px) {
		.kanban-board-surface {
			grid-auto-columns: minmax(14rem, 16rem);
		}
	}
</style>
