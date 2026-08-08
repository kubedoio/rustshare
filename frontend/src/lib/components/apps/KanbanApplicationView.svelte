<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import {
		listKanbanBoards,
		getKanbanBoard,
		getKanbanCard,
		createKanbanBoard,
		updateKanbanBoard,
		archiveKanbanBoard,
		createKanbanCard,
		updateKanbanCard,
		moveKanbanCard,
		archiveKanbanCard,
		deleteKanbanCard,
		createKanbanLabel,
		addCardLabel,
		removeCardLabel,
		getKanbanAssignableUsers,
		assignCardMember,
		unassignCardMember,
		saveKanbanCardDetail,
		addCardAttachment,
		deleteCardAttachment
	} from '$lib/api/kanban';
	import { goto } from '$app/navigation';
	import { resolveApplicationFolderId } from '$lib/applications/applicationPages';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import type { KanbanBoard, KanbanCard, KanbanCardDetail } from '$lib/api/types';
	import { currentUser } from '$lib/stores/auth';
	import { Folder as FolderIcon, Plus, ArrowLeft, MoreHorizontal } from 'lucide-svelte';

	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import CreateKanbanBoardModal from '$lib/components/modals/CreateKanbanBoardModal.svelte';
	import ApplicationPageShell from '$lib/components/layout/ApplicationPageShell.svelte';
	import type { ApplicationDefinition } from '$lib/applications/registry';

	import KanbanBoardList from './kanban/KanbanBoardList.svelte';
	import KanbanBoardView from './kanban/KanbanBoardView.svelte';
	import KanbanCardModal from './kanban/KanbanCardModal.svelte';

	interface Props {
		module: ApplicationDefinition;
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
	let cardDetail = $state<KanbanCardDetail | null>(null);
	let loadingDetail = $state(false);
	let savingDetail = $state(false);
	let saveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
	let draggingOverColumnId = $state<string | null>(null);
	let columnRefs = $state<Record<string, HTMLDivElement | null>>({});
	let viewMode = $state<'overview' | 'board'>('overview');
	let showBoardMenu = $state(false);
	let isMovingCard = $state(false);

	// Modal state
	let showPromptModal = $state(false);
	let promptTitle = $state('');
	let promptMessage = $state('');
	let promptDefaultValue = $state('');
	let promptConfirmLabel = $state('Confirm');
	let promptOnConfirm = $state<(value: string) => void>(() => {});

	let showConfirmModal = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmDanger = $state(false);
	let confirmOnConfirm = $state<() => void>(() => {});

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No boards yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ?? 'Create your first board to get started.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New board');

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
		if (selectedBoardId && boards.length > 0 && !boards.some((b) => b.id === selectedBoardId)) {
			selectedBoardId = '';
			viewMode = 'overview';
		}
	});

	const selectedBoard = $derived($boardQuery.data);

	let assignableUsersQuery = $derived(
		createQuery({
			queryKey: ['kanban-assignable-users'],
			queryFn: () => getKanbanAssignableUsers(),
			enabled: !!selectedBoardId
		})
	);

	let assignableUsers = $derived($assignableUsersQuery.data ?? []);

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

	// -------------------------------------------------------------------------
	// Board creation
	// -------------------------------------------------------------------------

	function handleCreateBoard() {
		showCreateBoardModal = true;
	}

	function handleBoardCreated(boardId: string) {
		selectedBoardId = boardId;
		viewMode = 'board';
		boardsQuery.refetch();
		showCreateBoardModal = false;
	}

	function selectBoard(boardId: string) {
		selectedBoardId = boardId;
		viewMode = 'board';
	}

	function showAllBoards() {
		selectedBoardId = '';
		viewMode = 'overview';
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveApplicationFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	async function handleRenameBoard() {
		if (!selectedBoard) return;
		promptTitle = 'Rename Board';
		promptMessage = 'Enter new board name:';
		promptDefaultValue = selectedBoard.title;
		promptConfirmLabel = 'Save';
		promptOnConfirm = async (newTitle: string) => {
			if (!newTitle || !selectedBoardId) return;
			try {
				await updateKanbanBoard(selectedBoardId, { title: newTitle.trim() });
				boardsQuery.refetch();
				boardQuery.refetch();
			} catch (err) {
				console.error('Failed to rename board:', err);
			}
		};
		showPromptModal = true;
	}

	function handleArchiveBoard() {
		if (!selectedBoardId) return;
		confirmTitle = 'Archive Board';
		confirmMessage = 'Archive this board?';
		confirmDanger = false;
		confirmOnConfirm = async () => {
			try {
				await archiveKanbanBoard(selectedBoardId);
				selectedBoardId = '';
				viewMode = 'overview';
				boardsQuery.refetch();
			} catch (err) {
				console.error('Failed to archive board:', err);
			}
		};
		showConfirmModal = true;
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
		if (!container) return 0;
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

	async function handleDrop(e: DragEvent, targetColumnId: string) {
		e.preventDefault();
		draggingOverColumnId = null;

		if (isMovingCard) return;
		if (!dragCardId || !dragSourceColumnId || !selectedBoard) return;

		const container = columnRefs[targetColumnId];
		if (!container) return;

		const dropIndex = getDropIndex(container, e.clientY);
		const targetColumn = selectedBoard.columns.find((c) => c.id === targetColumnId);
		if (!targetColumn) return;

		const otherCards = targetColumn.cards.filter((c) => c.id !== dragCardId);
		const beforeCardId = dropIndex > 0 ? otherCards[dropIndex - 1].id : undefined;
		const afterCardId = dropIndex < otherCards.length ? otherCards[dropIndex].id : undefined;

		// Don't move if dropped in same position
		if (targetColumnId === dragSourceColumnId) {
			const currentIndex = targetColumn.cards.findIndex((c) => c.id === dragCardId);
			if (currentIndex === dropIndex || currentIndex === dropIndex - 1) {
				handleDragEnd();
				return;
			}
		}

		const queryKey = ['kanban-board', selectedBoardId];
		const previousBoard = queryClient.getQueryData<KanbanBoard>(queryKey);

		try {
			// Optimistic update
			queryClient.setQueryData<KanbanBoard>(queryKey, (old) => {
				if (!old) return old;

				let movingCard: KanbanCard | undefined;
				const newColumns = old.columns.map((col) => {
					if (col.id === dragSourceColumnId) {
						movingCard = col.cards.find((c) => c.id === dragCardId);
						return { ...col, cards: col.cards.filter((c) => c.id !== dragCardId) };
					}
					return col;
				});

				if (!movingCard) return old;

				return {
					...old,
					columns: newColumns.map((col) => {
						if (col.id === targetColumnId) {
							const newCards = [...col.cards];
							newCards.splice(dropIndex, 0, { ...movingCard!, column_id: targetColumnId });
							return { ...col, cards: newCards };
						}
						return col;
					})
				};
			});

			isMovingCard = true;

			// API Call
			await moveKanbanCard(dragCardId, {
				boardId: selectedBoard.id,
				targetColumnId,
				beforeCardId,
				afterCardId
			});

			if (cardDetail && dragCardId === cardDetail.id) {
				const moveEvent = {
					event_type: 'card_moved',
					timestamp: new Date().toISOString(),
					actor: 'current-user',
					payload: { fromColumn: dragSourceColumnId, toColumn: targetColumnId },
					id: `act-${Date.now()}`,
					text: `Moved this card from ${dragSourceColumnId} to ${targetColumnId}`
				} as import('$lib/api/types').KanbanEvent;
				cardDetail.activity = [moveEvent, ...cardDetail.activity];
			}

			queryClient.invalidateQueries({ queryKey });
		} catch (err) {
			console.error('Failed to move card:', err);
			if (previousBoard) {
				queryClient.setQueryData(queryKey, previousBoard);
			}
			errorMessage = 'Card move failed. The board was restored to its previous state.';
		} finally {
			isMovingCard = false;
			handleDragEnd();
		}
	}

	// -------------------------------------------------------------------------
	// Card Operations
	// -------------------------------------------------------------------------

	function handleShowAddCard(columnId: string) {
		showCreateCardColumnId = columnId;
		newCardTitle = '';
	}

	function handleCreateCard(columnId: string, title: string) {
		if (!title.trim() || !selectedBoardId) return;
		createCardMutation.mutate({
			boardId: selectedBoardId,
			input: {
				title: title.trim(),
				column_id: columnId
			}
		});
	}

	function handleAddCardToFirstColumn() {
		const firstColumn = selectedBoard?.columns?.[0];
		if (firstColumn) {
			handleShowAddCard(firstColumn.id);
			return;
		}
		errorMessage = 'Add a column before creating cards.';
	}

	function handleCancelAddCard() {
		showCreateCardColumnId = null;
		newCardTitle = '';
	}

	function openCardEdit(card: KanbanCard) {
		editingCard = card;
		fetchCardDetail(card.id);
	}

	async function fetchCardDetail(cardId: string) {
		loadingDetail = true;
		try {
			cardDetail = await getKanbanCard(cardId);
		} catch (e) {
			console.error('Failed to fetch card detail', e);
		} finally {
			loadingDetail = false;
		}
	}

	async function saveCardDetail() {
		if (!cardDetail) return;
		savingDetail = true;
		saveStatus = 'saving';
		try {
			await saveKanbanCardDetail(cardDetail);
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
			saveStatus = 'saved';
			setTimeout(() => {
				if (saveStatus === 'saved') saveStatus = 'idle';
			}, 2000);
		} catch (e) {
			saveStatus = 'error';
		} finally {
			savingDetail = false;
		}
	}

	async function toggleLabel(labelId: string) {
		if (!cardDetail) return;
		const hasLabel = cardDetail.labels.some((l) => l.id === labelId);
		try {
			if (hasLabel) {
				await removeCardLabel(cardDetail.id, labelId);
				cardDetail.labels = cardDetail.labels.filter((l) => l.id !== labelId);
			} else {
				await addCardLabel(cardDetail.id, labelId);
				const label = selectedBoard?.labels.find((l) => l.id === labelId);
				if (label) {
					cardDetail.labels = [...cardDetail.labels, label];
				}
			}
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Failed to toggle label:', err);
		}
	}

	async function toggleAssignee(userId: string) {
		if (!cardDetail) return;
		const hasAssignee = cardDetail.assignees.some((a) => a.id === userId);
		try {
			if (hasAssignee) {
				await unassignCardMember(cardDetail.id, userId);
				cardDetail.assignees = cardDetail.assignees.filter((a) => a.id !== userId);
			} else {
				await assignCardMember(cardDetail.id, userId);
				const user = assignableUsers.find((u) => u.id === userId);
				if (user) {
					cardDetail.assignees = [...cardDetail.assignees, user];
				}
			}
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Failed to toggle assignee:', err);
		}
	}

	async function handleArchiveCard() {
		if (!editingCard) return;
		const cardId = editingCard.id;
		confirmTitle = 'Archive Card';
		confirmMessage = 'Archive this card?';
		confirmDanger = false;
		confirmOnConfirm = async () => {
			try {
				await archiveKanbanCard(cardId);
				boardQuery.refetch();
				editingCard = null;
				cardDetail = null;
			} catch (err) {
				console.error('Failed to archive card:', err);
			}
		};
		showConfirmModal = true;
	}

	async function handleDeleteCard() {
		if (!editingCard) return;
		const cardId = editingCard.id;
		confirmTitle = 'Delete Card';
		confirmMessage = 'Delete this card permanently?';
		confirmDanger = true;
		confirmOnConfirm = async () => {
			try {
				await deleteKanbanCard(cardId);
				boardQuery.refetch();
				editingCard = null;
				cardDetail = null;
			} catch (err) {
				console.error('Failed to delete card:', err);
			}
		};
		showConfirmModal = true;
	}

	async function handleCreateLabel(name: string, color: string) {
		if (!selectedBoardId || !name.trim()) return;
		try {
			const label = await createKanbanLabel(selectedBoardId, {
				name: name.trim(),
				color
			});
			// Label creation state is managed by the modal component
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
			toggleLabel(label.id);
		} catch (err) {
			console.error('Failed to create label:', err);
		}
	}

	async function handleAddAttachment(file: File) {
		if (!cardDetail) return;
		try {
			const attachment = await addCardAttachment(cardDetail.id, file);
			cardDetail.attachments = [...cardDetail.attachments, attachment];
			cardDetail.attachments_count = cardDetail.attachments.length;
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Failed to add attachment:', err);
		}
	}

	async function handleDeleteAttachment(attachmentId: string) {
		if (!cardDetail) return;
		try {
			await deleteCardAttachment(cardDetail.id, attachmentId);
			cardDetail.attachments = cardDetail.attachments.filter((a) => a.id !== attachmentId);
			cardDetail.attachments_count = cardDetail.attachments.length;
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Failed to delete attachment:', err);
		}
	}

	async function handleToggleChecklistItem(checklistId: string, itemId: string, done: boolean) {
		if (!cardDetail) return;
		// Update local state
		const checklists = cardDetail.checklists.map((cl) => {
			if (cl.id !== checklistId) return cl;
			return {
				...cl,
				items: cl.items.map((item) => {
					if (item.id !== itemId) return item;
					return { ...item, done };
				})
			};
		});
		cardDetail.checklists = checklists;
		// Update checklist summary
		let doneCount = 0;
		let totalCount = 0;
		for (const cl of checklists) {
			for (const item of cl.items) {
				totalCount++;
				if (item.done) doneCount++;
			}
		}
		cardDetail.checklist = { done: doneCount, total: totalCount };
	}

	async function handleAddChecklistItem(checklistId: string, text: string) {
		if (!cardDetail) return;
		const checklists = cardDetail.checklists.map((cl) => {
			if (cl.id !== checklistId) return cl;
			return {
				...cl,
				items: [...cl.items, { id: `item-${Date.now()}`, text, done: false }]
			};
		});
		cardDetail.checklists = checklists;
		// Recalculate checklist summary
		let doneCount = 0;
		let totalCount = 0;
		for (const cl of checklists) {
			for (const item of cl.items) {
				totalCount++;
				if (item.done) doneCount++;
			}
		}
		cardDetail.checklist = { done: doneCount, total: totalCount };
	}

	async function handleAddComment(text: string) {
		if (!cardDetail) return;
		const user = $currentUser;
		const newEvent = {
			event_type: 'comment',
			timestamp: new Date().toISOString(),
			actor: user?.id ?? 'unknown',
			payload: { text },
			id: `act-${Date.now()}`,
			text
		} as import('$lib/api/types').KanbanEvent;
		cardDetail.activity = [newEvent, ...cardDetail.activity];
	}
</script>

{#if $boardsQuery.isLoading}
	<div class="flex h-48 items-center justify-center">
		<div class="loading loading-md loading-spinner text-brand-500"></div>
	</div>
{:else if ($boardsQuery.data ?? []).length === 0}
	<ApplicationPageShell title="Kanban" subtitle="Manage file-backed boards and track work.">
		<div slot="primaryAction">
			<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New board</span>
			</button>
		</div>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
				<FolderIcon size={14} />
				<span>Open in Files</span>
			</button>
		</div>
		<EmptyState
			icon={'📋'}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	</ApplicationPageShell>
{:else if viewMode === 'overview'}
	<ApplicationPageShell title="Kanban" subtitle="Manage file-backed boards and track work.">
		<div slot="primaryAction">
			<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New board</span>
			</button>
		</div>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
				<FolderIcon size={14} />
				<span>Open in Files</span>
			</button>
		</div>

		<KanbanBoardList boards={$boardsQuery.data ?? []} onSelect={selectBoard} />
	</ApplicationPageShell>
{:else if viewMode === 'board'}
	<ApplicationPageShell
		title={selectedBoard?.title ?? 'Board'}
		breadcrumb={[
			{ label: module.displayName, onClick: showAllBoards },
			{ label: selectedBoard?.title ?? '' }
		]}
		metadata={selectedBoard
			? `${selectedBoard.columns.length} columns · ${selectedBoard.columns.reduce((sum, c) => sum + c.cards.length, 0)} cards`
			: ''}
	>
		<div slot="primaryAction">
			<button
				class="btn gap-2 btn-sm btn-primary"
				onclick={handleAddCardToFirstColumn}
				disabled={!selectedBoard || selectedBoard.columns.length === 0}
				title={!selectedBoard || selectedBoard.columns.length === 0
					? 'Add a column before creating cards'
					: 'Add card'}
			>
				<Plus size={14} />
				<span>Add card</span>
			</button>
		</div>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={showAllBoards}>
				<ArrowLeft size={14} />
				<span>All Boards</span>
			</button>
		</div>
		<div slot="overflowActions">
			<div class="relative">
				<button
					class="btn p-1 btn-ghost btn-sm"
					aria-label="Board menu"
					onclick={() => (showBoardMenu = !showBoardMenu)}
				>
					<MoreHorizontal size={18} />
				</button>
				{#if showBoardMenu}
					<div
						class="absolute top-full right-0 z-50 mt-1 min-w-[10rem] rounded-xl border border-base-300 bg-base-100 p-1 shadow-xl"
					>
						<button
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={() => {
								showBoardMenu = false;
								handleRenameBoard();
							}}
						>
							Rename board
						</button>
						<button
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={() => {
								showBoardMenu = false;
								handleOpenInFiles();
							}}
						>
							Open in Files
						</button>
						<button
							class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm text-error hover:bg-error/10"
							onclick={() => {
								showBoardMenu = false;
								handleArchiveBoard();
							}}
						>
							Archive board
						</button>
					</div>
				{/if}
			</div>
		</div>

		{#if errorMessage}
			<div class="rounded-lg border border-red-300 bg-red-50 px-4 py-2 text-sm text-red-700">
				{errorMessage}
			</div>
		{/if}

		{#if !selectedBoard || $boardQuery.isLoading}
			<div
				class="flex h-48 items-center justify-center rounded-3xl border border-base-300/40 bg-base-100"
			>
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else}
			<KanbanBoardView
				board={selectedBoard}
				onCardClick={openCardEdit}
				onShowAddCard={handleShowAddCard}
				onAddCard={handleCreateCard}
				onCancelAddCard={handleCancelAddCard}
				onNewCardTitleChange={(title) => (newCardTitle = title)}
				onDragStart={handleDragStart}
				onDragEnd={handleDragEnd}
				onDragOver={handleDragOver}
				onDragLeave={handleDragLeave}
				onDrop={handleDrop}
				{draggingOverColumnId}
				{showCreateCardColumnId}
				{newCardTitle}
				setColumnRef={(columnId, el) => (columnRefs[columnId] = el)}
			/>
		{/if}
	</ApplicationPageShell>
{/if}

<KanbanCardModal
	card={cardDetail}
	open={editingCard !== null}
	title={editingCard?.title || 'Edit Card'}
	board={selectedBoard ?? {
		id: '',
		title: '',
		slug: '',
		path: '',
		columns: [],
		labels: [],
		settings: {
			show_description_on_cards: true,
			description_preview_lines: 2,
			show_assignees: true,
			show_labels: true,
			show_due_date: true,
			show_attachment_badge: true,
			show_checklist_badge: true
		},
		created_at: '',
		updated_at: '',
		archived: false
	}}
	{assignableUsers}
	{loadingDetail}
	{savingDetail}
	{saveStatus}
	onClose={() => {
		editingCard = null;
		cardDetail = null;
	}}
	onSave={saveCardDetail}
	onArchive={handleArchiveCard}
	onDelete={handleDeleteCard}
	onToggleLabel={toggleLabel}
	onToggleAssignee={toggleAssignee}
	onCreateLabel={handleCreateLabel}
/>

<CreateKanbanBoardModal
	open={showCreateBoardModal}
	onClose={() => (showCreateBoardModal = false)}
	onSuccess={handleBoardCreated}
	defaultTemplate={module.defaultTemplate}
	existingNames={($boardsQuery.data ?? []).map((b) => b.title)}
/>

<PromptModal
	open={showPromptModal}
	title={promptTitle}
	message={promptMessage}
	defaultValue={promptDefaultValue}
	confirmLabel={promptConfirmLabel}
	onConfirm={(value) => {
		showPromptModal = false;
		promptOnConfirm(value);
	}}
	onCancel={() => (showPromptModal = false)}
/>

<ConfirmModal
	open={showConfirmModal}
	title={confirmTitle}
	message={confirmMessage}
	confirmLabel="Confirm"
	cancelLabel="Cancel"
	danger={confirmDanger}
	onConfirm={() => {
		showConfirmModal = false;
		confirmOnConfirm();
	}}
	onCancel={() => (showConfirmModal = false)}
/>
