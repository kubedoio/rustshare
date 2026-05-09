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
		addCardAttachment,
		deleteCardAttachment,
		createChecklist,
		createChecklistItem,
		toggleChecklistItem,
		deleteChecklistItem,
		deleteChecklist
	} from '$lib/api/kanban';
	import { goto } from '$app/navigation';
	import { createFromTemplate } from '$lib/api/modules';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import type { KanbanBoard, KanbanCard, KanbanCardDetail } from '$lib/api/types';
	import {
		Folder as FolderIcon,
		Plus,
		ArrowLeft,
		MoreHorizontal
	} from 'lucide-svelte';

	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import CreateKanbanBoardModal from '$lib/components/modals/CreateKanbanBoardModal.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	import KanbanBoardList from './kanban/KanbanBoardList.svelte';
	import KanbanBoardView from './kanban/KanbanBoardView.svelte';
	import KanbanCardModal from './kanban/KanbanCardModal.svelte';

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
	let cardDetail = $state<KanbanCardDetail | null>(null);
	let loadingDetail = $state(false);
	let savingDetail = $state(false);
	let saveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
	let draggingOverColumnId = $state<string | null>(null);
	let columnRefs = $state<Record<string, HTMLDivElement | null>>({});
	let viewMode = $state<'overview' | 'board'>('overview');
	let showBoardMenu = $state(false);
	let isMovingCard = $state(false);
	let uploadingAttachment = $state(false);

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

	const moveCardMutation = createMutation({
		mutationFn: ({
			cardId,
			input
		}: {
			cardId: string;
			input: {
				boardId: string;
				targetColumnId: string;
				targetOrder?: number;
				beforeCardId?: string;
				afterCardId?: string;
			};
		}) => moveKanbanCard(cardId, input),
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
			const folderId = await resolveModuleFolderId(module.rootPath);
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
			await updateKanbanCard(cardDetail.id, {
				title: cardDetail.title,
				content: cardDetail.content
			});
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

	function handleArchiveCard() {
		if (!editingCard) return;
		const cardId = editingCard.id;
		confirmTitle = 'Archive Card';
		confirmMessage = 'Archive this card?';
		confirmDanger = false;
		confirmOnConfirm = () => {
			archiveCardMutation.mutate(cardId);
		};
		showConfirmModal = true;
	}

	function handleDeleteCard() {
		if (!editingCard) return;
		const cardId = editingCard.id;
		confirmTitle = 'Delete Card';
		confirmMessage = 'Delete this card permanently?';
		confirmDanger = true;
		confirmOnConfirm = () => {
			deleteCardMutation.mutate(cardId);
		};
		showConfirmModal = true;
	}

	async function handleFileUpload(e: Event) {
		const target = e.target as HTMLInputElement;
		const file = target.files?.[0];
		if (!file || !cardDetail) return;

		uploadingAttachment = true;
		try {
			const attachment = await addCardAttachment(cardDetail.id, file);
			cardDetail.attachments = [...cardDetail.attachments, attachment];
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Upload failed', err);
		} finally {
			uploadingAttachment = false;
			target.value = '';
		}
	}

	let pendingAttachmentId = $state('');

	async function deleteAttachment(attachmentId: string) {
		if (!cardDetail) return;
		pendingAttachmentId = attachmentId;
		confirmTitle = 'Delete Attachment';
		confirmMessage = 'Delete this attachment?';
		confirmDanger = true;
		confirmOnConfirm = async () => {
			if (!cardDetail) return;
			try {
				await deleteCardAttachment(cardDetail.id, pendingAttachmentId);
				cardDetail.attachments = cardDetail.attachments.filter((a) => a.id !== pendingAttachmentId);
				queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
			} catch (err) {
				console.error('Delete attachment failed', err);
			}
		};
		showConfirmModal = true;
	}

	async function handleAddChecklist(title: string) {
		if (!cardDetail || !title.trim()) return;
		try {
			const group = await createChecklist(cardDetail.id, title.trim());
			cardDetail.checklists = [...cardDetail.checklists, group];
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Add checklist failed', err);
		}
	}

	async function handleAddChecklistItem(checklistId: string, text: string) {
		if (!cardDetail || !text.trim()) return;
		try {
			const item = await createChecklistItem(cardDetail.id, checklistId, text.trim());
			cardDetail.checklists = cardDetail.checklists.map((c) => {
				if (c.id === checklistId) {
					return { ...c, items: [...c.items, item] };
				}
				return c;
			});
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Add checklist item failed', err);
		}
	}

	async function handleToggleItem(checklistId: string, itemId: string, done: boolean) {
		if (!cardDetail) return;
		try {
			await toggleChecklistItem(cardDetail.id, checklistId, itemId, done);
			cardDetail.checklists = cardDetail.checklists.map((c) => {
				if (c.id === checklistId) {
					return {
						...c,
						items: c.items.map((i) => (i.id === itemId ? { ...i, done } : i))
					};
				}
				return c;
			});
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Toggle item failed', err);
		}
	}

	async function handleDeleteItem(checklistId: string, itemId: string) {
		if (!cardDetail) return;
		try {
			await deleteChecklistItem(cardDetail.id, checklistId, itemId);
			cardDetail.checklists = cardDetail.checklists.map((c) => {
				if (c.id === checklistId) {
					return { ...c, items: c.items.filter((i) => i.id !== itemId) };
				}
				return c;
			});
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Delete item failed', err);
		}
	}

	let pendingChecklistId = $state('');

	async function handleDeleteChecklist(checklistId: string) {
		if (!cardDetail) return;
		pendingChecklistId = checklistId;
		confirmTitle = 'Delete Checklist';
		confirmMessage = 'Delete this checklist?';
		confirmDanger = true;
		confirmOnConfirm = async () => {
			if (!cardDetail) return;
			try {
				await deleteChecklist(cardDetail.id, pendingChecklistId);
				cardDetail.checklists = cardDetail.checklists.filter((c) => c.id !== pendingChecklistId);
				queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
			} catch (err) {
				console.error('Delete checklist failed', err);
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
</script>

{#if $boardsQuery.isLoading}
	<div class="flex h-48 items-center justify-center">
		<div class="loading loading-md loading-spinner text-brand-500"></div>
	</div>
{:else if ($boardsQuery.data ?? []).length === 0}
	<ModulePageShell title="Kanban" subtitle="Manage file-backed boards and track work.">
		<div slot="primaryAction">
			<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
				<FolderIcon size={14} />
				<span>Open in Files</span>
			</button>
		</div>
		<EmptyState
			icon={"📋"}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	</ModulePageShell>
{:else if viewMode === 'overview'}
	<ModulePageShell title="Kanban" subtitle="Manage file-backed boards and track work.">
		<div slot="primaryAction">
			<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>
		<div slot="secondaryActions">
			<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
				<FolderIcon size={14} />
				<span>Open in Files</span>
			</button>
		</div>

		<KanbanBoardList boards={$boardsQuery.data ?? []} onSelect={selectBoard} />
	</ModulePageShell>
{:else if viewMode === 'board'}
	<ModulePageShell
		title={selectedBoard?.title ?? 'Board'}
		breadcrumb={[{ label: 'Kanban', onClick: showAllBoards }, { label: selectedBoard?.title ?? '' }]}
		metadata={selectedBoard
			? `${selectedBoard.columns.length} columns · ${selectedBoard.columns.reduce((sum, c) => sum + c.cards.length, 0)} cards`
			: ''}
	>
		<div slot="primaryAction">
			<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
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
				draggingOverColumnId={draggingOverColumnId}
				showCreateCardColumnId={showCreateCardColumnId}
				newCardTitle={newCardTitle}
				setColumnRef={(columnId, el) => (columnRefs[columnId] = el)}
			/>
		{/if}
	</ModulePageShell>
{/if}

<KanbanCardModal
	card={cardDetail}
	open={editingCard !== null}
	title={editingCard?.title || 'Edit Card'}
	board={selectedBoard ?? { id: '', title: '', slug: '', path: '', columns: [], labels: [], settings: { show_description_on_cards: true, description_preview_lines: 2, show_assignees: true, show_labels: true, show_due_date: true, show_attachment_badge: true, show_checklist_badge: true }, created_at: '', updated_at: '', archived: false }}
	{assignableUsers}
	{loadingDetail}
	{savingDetail}
	{saveStatus}
	{uploadingAttachment}
	onClose={() => {
		editingCard = null;
		cardDetail = null;
	}}
	onSave={saveCardDetail}
	onArchive={handleArchiveCard}
	onDelete={handleDeleteCard}
	onToggleLabel={toggleLabel}
	onToggleAssignee={toggleAssignee}
	onUploadAttachment={handleFileUpload}
	onDeleteAttachment={deleteAttachment}
	onAddChecklist={handleAddChecklist}
	onDeleteChecklist={handleDeleteChecklist}
	onAddChecklistItem={handleAddChecklistItem}
	onToggleItem={handleToggleItem}
	onDeleteItem={handleDeleteItem}
	onCreateLabel={handleCreateLabel}
/>

<CreateKanbanBoardModal
	open={showCreateBoardModal}
	onClose={() => (showCreateBoardModal = false)}
	onSuccess={handleBoardCreated}
	defaultTemplate={module.defaultTemplate}
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
