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
	import type { KanbanBoard, KanbanCard, KanbanCardDetail, KanbanColumn } from '$lib/api/types';
	import {
		Folder as FolderIcon,
		Plus,
		GripVertical,
		Archive,
		Trash2,
		X,
		ChevronRight,
		Paperclip,
		CheckSquare,
		Calendar,
		User,
		Save,
		Clock,
		Tag,
		AlignLeft,
		Activity,
		Check,
		Columns,
		MoreHorizontal,
		ArrowLeft
	} from 'lucide-svelte';

	import RichMarkdownEditor from '../../editor/components/RichMarkdownEditor.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import CreateKanbanBoardModal from '$lib/components/modals/CreateKanbanBoardModal.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
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
	let cardDetail = $state<KanbanCardDetail | null>(null);
	let loadingDetail = $state(false);
	let savingDetail = $state(false);
	let saveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
	let draggingOverColumnId = $state<string | null>(null);
	let columnRefs = $state<Record<string, HTMLDivElement | null>>({});
	let viewMode = $state<'overview' | 'board'>('overview');
	let showLabelPicker = $state(false);
	let showAssigneePicker = $state(false);
	let showNewLabelForm = $state(false);
	let newLabelName = $state('');
	let newLabelColor = $state('blue');
	let uploadingAttachment = $state(false);
	let isMovingCard = $state(false);
	let newChecklistTitle = $state('');
	let newChecklistItemText = $state<Record<string, string>>({});
	let showBoardMenu = $state(false);

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
	// Helpers
	// -------------------------------------------------------------------------

	function formatColumnName(value: string): string {
		return value.replace(/^\d+-/, '').replace(/-/g, ' ');
	}

	function get_initials(name: string): string {
		if (!name) return '??';
		return name
			.split(' ')
			.filter((n) => n.length > 0)
			.map((n) => n[0])
			.join('')
			.toUpperCase()
			.substring(0, 2);
	}

	function formatDate(dateStr: string) {
		const date = new Date(dateStr);
		const now = new Date();
		const isSameYear = date.getFullYear() === now.getFullYear();

		return new Intl.DateTimeFormat('en-US', {
			month: 'short',
			day: 'numeric',
			year: isSameYear ? undefined : 'numeric'
		}).format(date);
	}

	function isOverdue(dateStr: string) {
		const date = new Date(dateStr);
		const now = new Date();
		now.setHours(0, 0, 0, 0);
		return date < now;
	}

	function formatActivityDate(dateStr: string) {
		const date = new Date(dateStr);
		return new Intl.DateTimeFormat('en-US', {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		}).format(date);
	}

	const EVENT_LABELS: Record<string, string> = {
		'card.created': 'Card created',
		'card.moved': 'Card moved',
		'card.title_changed': 'Title updated',
		'card.description_changed': 'Description updated',
		'card.label_added': 'Label added',
		'card.label_removed': 'Label removed',
		'card.assignee_added': 'Assignee added',
		'card.assignee_removed': 'Assignee removed',
		'card.checklist_added': 'Checklist added',
		'card.checklist_item_added': 'Checklist item added',
		'card.checklist_item_toggled': 'Checklist item completed',
		'card.attachment_added': 'Attachment added',
		'card.attachment_removed': 'Attachment removed',
		'card.due_date_changed': 'Due date updated',
		'card.archived': 'Card archived',
		'board.created': 'Board created',
		'board.renamed': 'Board renamed',
		'board.column_added': 'Column added'
	};

	function getEventLabel(eventType: string): string {
		return (
			EVENT_LABELS[eventType] ||
			eventType.replace(/card\./, '').replace(/\./g, ' ').replace(/_/g, ' ')
		);
	}

	// -------------------------------------------------------------------------
	// Card Operations
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

	async function handleAddChecklist() {
		if (!cardDetail || !newChecklistTitle.trim()) return;
		try {
			const group = await createChecklist(cardDetail.id, newChecklistTitle.trim());
			cardDetail.checklists = [...cardDetail.checklists, group];
			newChecklistTitle = '';
			queryClient.invalidateQueries({ queryKey: ['kanban-board', selectedBoardId] });
		} catch (err) {
			console.error('Add checklist failed', err);
		}
	}

	async function handleAddChecklistItem(checklistId: string) {
		const text = newChecklistItemText[checklistId];
		if (!cardDetail || !text?.trim()) return;
		try {
			const item = await createChecklistItem(cardDetail.id, checklistId, text.trim());
			cardDetail.checklists = cardDetail.checklists.map((c) => {
				if (c.id === checklistId) {
					return { ...c, items: [...c.items, item] };
				}
				return c;
			});
			newChecklistItemText[checklistId] = '';
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

	async function handleCreateLabel() {
		if (!selectedBoardId || !newLabelName.trim()) return;
		try {
			const label = await createKanbanLabel(selectedBoardId, {
				name: newLabelName.trim(),
				color: newLabelColor
			});
			newLabelName = '';
			showNewLabelForm = false;
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

		<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
			{#each $boardsQuery.data ?? [] as board}
				<button
					type="button"
					class="group flex flex-col gap-3 rounded-xl border border-base-300/40 bg-base-100 p-5 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					onclick={() => selectBoard(board.id)}
				>
					<div class="flex items-start justify-between">
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<Columns size={18} />
						</div>
					</div>
					<div class="flex flex-col gap-1">
						<span class="text-sm font-semibold text-base-content">{board.title}</span>
						<span class="text-xs text-base-content/50">
							{board.column_count} column{board.column_count === 1 ? '' : 's'} · {board.card_count} card{board.card_count ===
							1
								? ''
								: 's'}
						</span>
						<span class="text-xs text-base-content/40">
							Updated {formatDate(board.updated_at)}
						</span>
					</div>
				</button>
			{/each}
		</div>
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
							<div class="flex items-center gap-2">
								<span>{column.cards.length}</span>
								<button
									class="text-base-content/40 hover:text-base-content"
									aria-label="Add card to {formatColumnName(column.title)}"
									onclick={() => {
										showCreateCardColumnId = column.id;
										newCardTitle = '';
									}}
								>
									<Plus size={14} />
								</button>
							</div>
						</header>

						<div class="kanban-card-list" bind:this={columnRefs[column.id]}>
							{#if column.cards.length === 0}
								<div class="kanban-empty-column">
									No cards yet
									<button
										class="mt-1 block text-xs text-brand-500 hover:underline"
										onclick={() => {
											showCreateCardColumnId = column.id;
											newCardTitle = '';
										}}
									>
										Add card
									</button>
								</div>
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

									{#if card.description_preview}
										<div class="card-description">
											{card.description_preview}
										</div>
									{/if}

									{#if (card.labels && card.labels.length > 0) || card.priority !== 'normal'}
										<div class="card-labels">
											{#if card.priority !== 'normal'}
												<span class="card-label label-priority priority-{card.priority}">
													{card.priority}
												</span>
											{/if}
											{#each card.labels.slice(0, 3) as label}
												<span class="card-label label-{label.color}">
													{label.name}
												</span>
											{/each}
											{#if card.labels.length > 3}
												<span class="card-label label-more">+{card.labels.length - 3}</span>
											{/if}
										</div>
									{/if}

									<div class="card-footer">
										<div class="card-badges">
											{#if card.attachments_count > 0}
												<span class="card-badge" title="Attachments">
													<Paperclip size={12} />
													{card.attachments_count}
												</span>
											{/if}
											{#if card.checklist.total > 0}
												<span
													class="card-badge"
													title="Checklist"
													class:badge-done={card.checklist.done === card.checklist.total}
												>
													<CheckSquare size={12} />
													{card.checklist.done}/{card.checklist.total}
												</span>
											{/if}
											{#if card.due_date}
												<span
													class="card-badge"
													title="Due Date"
													class:badge-overdue={isOverdue(card.due_date)}
												>
													<Calendar size={12} />
													{formatDate(card.due_date)}
												</span>
											{/if}
										</div>

										{#if card.assignees && card.assignees.length > 0}
											<div class="card-assignees">
												{#each card.assignees.slice(0, 3) as assignee}
													<div class="assignee-avatar" title={assignee.display_name}>
														{#if assignee.avatar_url}
															<img src={assignee.avatar_url} alt={assignee.display_name} />
														{:else}
															<span>{assignee.initials}</span>
														{/if}
													</div>
												{/each}
												{#if card.assignees.length > 3}
													<div class="assignee-avatar avatar-more" title="More assignees">
														<span>+{card.assignees.length - 3}</span>
													</div>
												{/if}
											</div>
										{/if}
									</div>
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
	</ModulePageShell>
{/if}

<!-- Card edit modal -->
<ModalBase
	open={editingCard !== null}
	onClose={() => {
		editingCard = null;
		cardDetail = null;
	}}
	title={editingCard?.title || 'Edit Card'}
>
	<div class="card-detail-drawer">
		{#if loadingDetail}
			<div class="flex h-64 items-center justify-center">
				<span class="loading loading-lg loading-spinner text-brand-500"></span>
			</div>
		{:else if cardDetail}
			<header class="detail-header">
				<div class="header-main">
					<div class="title-row">
						<input
							type="text"
							bind:value={cardDetail.title}
							class="detail-title-input"
							placeholder="Card Title"
							onblur={saveCardDetail}
						/>
					</div>
					<div class="detail-meta">
						in column <span class="font-bold">{cardDetail.status}</span>
					</div>
				</div>
				<div class="header-actions">
					{#if saveStatus === 'saving'}
						<span class="text-xs text-base-content/50">Saving...</span>
					{:else if saveStatus === 'saved'}
						<span class="text-xs text-green-600">Saved</span>
					{:else if saveStatus === 'error'}
						<span class="text-xs text-red-600">Error saving</span>
					{/if}
					<button class="btn-close" onclick={() => (editingCard = null)}>
						<X size={20} />
					</button>
				</div>
			</header>

			<div class="detail-content">
				<div class="detail-main">
					<!-- Labels & Assignees -->
					<div class="detail-badges-row">
						<div class="detail-section">
							<h4 class="section-label">Labels</h4>
							<div class="flex flex-wrap items-center gap-1">
								{#each cardDetail.labels as label}
									<span class="card-label label-{label.color} group relative">
										{label.name}
										<button
											class="absolute -top-1.5 -right-1.5 hidden h-4 w-4 items-center justify-center rounded-full bg-base-content text-base-100 shadow-sm group-hover:flex"
											onclick={() => toggleLabel(label.id)}
										>
											<X size={10} />
										</button>
									</span>
								{/each}
								<div class="relative">
									<button
										class="btn h-7 rounded-lg border border-dashed border-base-300 px-2 btn-ghost btn-xs hover:border-brand-500"
										onclick={() => {
											showLabelPicker = !showLabelPicker;
											showAssigneePicker = false;
										}}
									>
										<Plus size={12} class="mr-1" />
										<span>Add</span>
									</button>
									{#if showLabelPicker && selectedBoard}
										<div
											class="absolute top-full left-0 z-50 mt-1 min-w-[200px] rounded-xl border border-base-300 bg-base-100 p-3 shadow-xl"
										>
											<div class="mb-2 flex items-center justify-between">
												<div class="text-[10px] font-bold text-base-content/40 uppercase">
													Select Label
												</div>
												<button
													class="btn h-6 px-1 text-brand-500 btn-ghost btn-xs hover:bg-brand-50"
													onclick={() => (showNewLabelForm = !showNewLabelForm)}
												>
													{showNewLabelForm ? 'Cancel' : 'New'}
												</button>
											</div>

											{#if showNewLabelForm}
												<div class="mb-3 flex flex-col gap-2 rounded-lg bg-base-200/50 p-2">
													<input
														type="text"
														placeholder="Label name..."
														class="input input-xs h-8 bg-base-100"
														bind:value={newLabelName}
														onkeydown={(e) => e.key === 'Enter' && handleCreateLabel()}
													/>
													<div class="flex flex-wrap gap-1">
														{#each ['green', 'yellow', 'orange', 'red', 'purple', 'blue', 'gray'] as color}
															<button
																aria-label={color}
																class="h-5 w-5 rounded-full label-{color} border-2 {newLabelColor ===
																color
																	? 'border-base-content'
																	: 'border-transparent'}"
																onclick={() => (newLabelColor = color)}
															></button>
														{/each}
													</div>
													<button
														class="btn h-8 w-full btn-xs btn-primary"
														disabled={!newLabelName.trim()}
														onclick={handleCreateLabel}
													>
														Create Label
													</button>
												</div>
											{/if}

											<div class="flex max-h-48 flex-col gap-1 overflow-y-auto">
												{#each selectedBoard.labels as label}
													<button
														class="flex items-center justify-between rounded-lg px-2 py-1.5 text-left text-xs hover:bg-base-200"
														onclick={() => toggleLabel(label.id)}
													>
														<span class="card-label label-{label.color} !m-0">{label.name}</span>
														{#if cardDetail.labels.some((l) => l.id === label.id)}
															<Check size={12} class="text-brand-500" />
														{/if}
													</button>
												{/each}
											</div>
										</div>
									{/if}
								</div>
							</div>
						</div>

						<div class="detail-section">
							<h4 class="section-label">Assignees</h4>
							<div class="flex flex-wrap items-center gap-1">
								{#each cardDetail.assignees as assignee}
									<div class="assignee-avatar group relative" title={assignee.display_name}>
										{#if assignee.avatar_url}
											<img src={assignee.avatar_url} alt={assignee.display_name} />
										{:else}
											<span>{assignee.initials}</span>
										{/if}
										<button
											class="absolute -top-1 -right-1 z-10 hidden h-4 w-4 items-center justify-center rounded-full bg-base-content text-base-100 shadow-sm group-hover:flex"
											onclick={() => toggleAssignee(assignee.id)}
										>
											<X size={10} />
										</button>
									</div>
								{/each}
								<div class="relative">
									<button
										class="btn h-8 w-8 rounded-full border border-dashed border-base-300 p-0 btn-ghost btn-xs hover:border-brand-500"
										onclick={() => {
											showAssigneePicker = !showAssigneePicker;
											showLabelPicker = false;
										}}
									>
										<Plus size={14} />
									</button>
									{#if showAssigneePicker}
										<div
											class="absolute top-full left-0 z-50 mt-1 min-w-[200px] rounded-xl border border-base-300 bg-base-100 p-2 shadow-xl"
										>
											<div class="mb-1 px-2 text-[10px] font-bold text-base-content/40 uppercase">
												Assign Member
											</div>
											<div class="flex max-h-48 flex-col gap-1 overflow-y-auto">
												{#each assignableUsers as user}
													<button
														class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs hover:bg-base-200"
														onclick={() => toggleAssignee(user.id)}
													>
														<div class="assignee-avatar !h-6 !w-6 !text-[10px]">
															{#if user.avatar_url}
																<img src={user.avatar_url} alt={user.display_name} />
															{:else}
																<span>{user.initials}</span>
															{/if}
														</div>
														<span class="flex-1 truncate">{user.display_name}</span>
														{#if cardDetail.assignees.some((a) => a.id === user.id)}
															<Check size={12} class="text-brand-500" />
														{/if}
													</button>
												{/each}
											</div>
										</div>
									{/if}
								</div>
							</div>
						</div>
					</div>

					<!-- Description -->
					<div class="detail-section">
						<div class="mb-2 flex items-center gap-2">
							<AlignLeft size={18} class="text-base-content/60" />
							<h4 class="section-label !mb-0">Description</h4>
						</div>
						<div class="description-editor">
							<RichMarkdownEditor
								bind:content={cardDetail.content}
								editable={true}
								on:change={() => {
									saveStatus = 'idle';
								}}
								hasAttachmentHandler={true}
							/>
							<div class="mt-2 flex justify-end">
								<button
									class="btn btn-sm btn-primary"
									disabled={savingDetail}
									onclick={saveCardDetail}
								>
									{#if savingDetail}
										<span class="loading loading-xs loading-spinner"></span>
									{/if}
									Save Changes
								</button>
							</div>
						</div>
					</div>

					<!-- Attachments -->
					<div class="detail-section">
						<div class="mb-4 flex items-center justify-between">
							<div class="flex items-center gap-2">
								<Paperclip size={18} class="text-base-content/60" />
								<h4 class="section-label !mb-0">Attachments</h4>
							</div>
							<label class="btn gap-1 btn-ghost btn-xs">
								<Plus size={14} />
								Add
								<input type="file" class="hidden" onchange={handleFileUpload} />
							</label>
						</div>

						{#if cardDetail.attachments.length > 0}
							<div class="grid grid-cols-1 gap-2">
								{#each cardDetail.attachments as attachment}
									<div class="attachment-item group">
										<div class="attachment-icon">
											<FolderIcon size={16} />
										</div>
										<div class="attachment-info flex-1">
											<div class="attachment-name">{attachment.name}</div>
											<div class="attachment-meta">
												{Math.round(attachment.size / 1024)} KB • {formatActivityDate(
													attachment.created_at
												)}
											</div>
										</div>
										<button
											class="btn text-error opacity-0 btn-ghost btn-xs group-hover:opacity-100"
											onclick={() => deleteAttachment(attachment.id)}
										>
											<Trash2 size={14} />
										</button>
									</div>
								{/each}
							</div>
						{:else}
							<div class="px-2 text-xs text-base-content/40 italic">No attachments yet.</div>
						{/if}

						{#if uploadingAttachment}
							<div class="mt-2 flex items-center gap-2 text-xs text-base-content/60">
								<span class="loading loading-xs loading-spinner"></span>
								Uploading...
							</div>
						{/if}
					</div>

					<!-- Checklists -->
					<div class="detail-section">
						<div class="mb-4 flex items-center justify-between">
							<div class="flex items-center gap-2">
								<CheckSquare size={18} class="text-base-content/60" />
								<h4 class="section-label !mb-0">Checklists</h4>
							</div>
							<div class="flex items-center gap-2">
								<input
									type="text"
									placeholder="New checklist..."
									class="input-bordered input input-xs w-32"
									bind:value={newChecklistTitle}
									onkeydown={(e) => e.key === 'Enter' && handleAddChecklist()}
								/>
								<button class="btn btn-xs btn-primary" onclick={handleAddChecklist}>Add</button>
							</div>
						</div>

						{#each cardDetail.checklists as checklist}
							<div class="mb-6 rounded-xl border border-base-200/50 bg-base-200/20 p-4 last:mb-0">
								<div class="mb-2 flex items-center justify-between">
									<h5 class="flex items-center gap-2 text-sm font-bold">
										{checklist.title}
										<span
											class="rounded-full bg-base-200 px-1.5 py-0.5 text-[10px] font-medium text-base-content/60"
										>
											{checklist.items.filter((i) => i.done).length}/{checklist.items.length}
										</span>
									</h5>
									<button
										class="btn text-error/40 btn-ghost btn-xs hover:text-error"
										onclick={() => handleDeleteChecklist(checklist.id)}
									>
										<Trash2 size={12} />
									</button>
								</div>

								<div class="mb-3 h-1.5 w-full overflow-hidden rounded-full bg-base-200">
									<div
										class="h-full bg-success transition-all duration-300"
										style="width: {(checklist.items.filter((i) => i.done).length /
											(checklist.items.length || 1)) *
											100}%"
									></div>
								</div>

								<div class="mb-3 flex flex-col gap-1">
									{#each checklist.items as item}
										<div
											class="group flex items-center gap-2 rounded-lg p-1 transition-colors hover:bg-base-200/50"
										>
											<input
												type="checkbox"
												checked={item.done}
												class="checkbox checkbox-xs checkbox-primary"
												onchange={(e) =>
													handleToggleItem(
														checklist.id,
														item.id,
														(e.target as HTMLInputElement).checked
													)}
											/>
											<span
												class="flex-1 text-sm"
												class:line-through={item.done}
												class:opacity-50={item.done}
											>
												{item.text}
											</span>
											<button
												class="btn text-base-content/20 opacity-0 btn-ghost btn-xs group-hover:opacity-100 hover:text-error"
												onclick={() => handleDeleteItem(checklist.id, item.id)}
											>
												<X size={12} />
											</button>
										</div>
									{/each}
								</div>

								<div class="flex items-center gap-2">
									<input
										type="text"
										placeholder="Add an item..."
										class="input-bordered input input-xs flex-1"
										bind:value={newChecklistItemText[checklist.id]}
										onkeydown={(e) => e.key === 'Enter' && handleAddChecklistItem(checklist.id)}
									/>
									<button
										class="btn btn-ghost btn-xs"
										onclick={() => handleAddChecklistItem(checklist.id)}
									>
										Add
									</button>
								</div>
							</div>
						{/each}

						{#if cardDetail.checklists.length === 0}
							<div class="px-2 text-xs text-base-content/40 italic">No checklists yet.</div>
						{/if}
					</div>

					<!-- Activity -->
					<div class="detail-section">
						<div class="mb-4 flex items-center gap-2">
							<Activity size={18} class="text-base-content/60" />
							<h4 class="section-label !mb-0">Activity</h4>
						</div>
						<div class="activity-feed">
							{#each cardDetail.activity as event}
								<div class="activity-item">
									<div class="activity-avatar">
										{event.actor.charAt(0).toUpperCase()}
									</div>
									<div class="activity-body">
										<div class="activity-header">
											<span class="font-bold">{event.actor}</span>
											<span class="ml-1 text-base-content/40"
												>{formatActivityDate(event.timestamp)}</span
											>
										</div>
										<div class="activity-text">
											{getEventLabel(event.event_type)}
										</div>
									</div>
								</div>
							{/each}
						</div>
					</div>
				</div>
			</div>
		{/if}
	</div>
</ModalBase>

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
		align-items: flex-start;
		gap: 0.4rem;
	}

	.kanban-card-title-row :global(svg) {
		margin-top: 0.15rem;
		flex-shrink: 0;
	}

	.card-description {
		margin-top: 0.15rem;
		padding-left: 1.25rem;
		font-size: 0.72rem;
		line-height: 1.35;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.card-labels {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		margin-top: 0.5rem;
		padding-left: 1.25rem;
	}

	.card-label {
		font-size: 0.6rem;
		font-weight: 700;
		padding: 0.1rem 0.4rem;
		border-radius: 0.25rem;
		text-transform: uppercase;
		letter-spacing: 0.02em;
	}

	.label-green {
		background: #61bd4f;
		color: white;
	}
	.label-yellow {
		background: #f2d600;
		color: #42526e;
	}
	.label-orange {
		background: #ff9f1a;
		color: white;
	}
	.label-red {
		background: #eb5a46;
		color: white;
	}
	.label-purple {
		background: #c377e0;
		color: white;
	}
	.label-blue {
		background: #0079bf;
		color: white;
	}
	.label-gray {
		background: #b3bac5;
		color: white;
	}
	.label-more {
		background: var(--base-200);
		color: var(--base-content);
		opacity: 0.8;
	}

	.priority-urgent {
		background: #eb5a46;
		color: white;
		box-shadow: 0 0 4px rgba(235, 90, 70, 0.4);
	}
	.priority-high {
		background: #ff9f1a;
		color: white;
	}
	.priority-low {
		background: #b3bac5;
		color: white;
	}

	.card-footer {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-top: 0.75rem;
		padding-left: 1.25rem;
		flex-wrap: wrap;
	}

	.card-badges {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
	}

	.card-badge {
		display: flex;
		align-items: center;
		gap: 0.2rem;
		font-size: 0.68rem;
		font-weight: 600;
	}

	.badge-done {
		color: #61bd4f;
	}

	.badge-overdue {
		color: #eb5a46;
		background: color-mix(in oklab, #eb5a46 8%, transparent);
		padding: 0 0.25rem;
		border-radius: 0.25rem;
	}

	.card-assignees {
		display: flex;
		/* Avatars stack from left to right now to match the "SC" text-like flow */
	}

	.assignee-avatar {
		width: 1.4rem;
		height: 1.4rem;
		border-radius: 999px;
		background: var(--base-200);
		border: 1.5px solid var(--rs-surface-primary, white);
		margin-right: -0.3rem; /* Stack slightly to the right */
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.55rem;
		font-weight: 700;
		color: var(--base-content);
		overflow: hidden;
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
	}

	.assignee-avatar img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.avatar-more {
		background: var(--base-300);
		color: var(--base-content);
	}

	.kanban-card strong {
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
		line-height: 1.4;
	}

	.kanban-empty-column {
		border: 1px dashed color-mix(in oklab, var(--base-300) 60%, transparent);
		border-radius: 1rem;
		background: rgba(255, 255, 255, 0.45);
		padding: 1rem;
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}

	.card-detail-drawer {
		width: 100%;
		max-width: 48rem;
		min-height: 30rem;
		max-height: 90vh;
		overflow-y: auto;
		background: var(--rs-surface-primary, white);
		display: flex;
		flex-direction: column;
	}

	.detail-header {
		display: flex;
		justify-content: space-between;
		padding: 1.5rem;
		background: color-mix(in oklab, var(--base-100) 95%, black);
		border-bottom: 1px solid var(--base-200);
		position: sticky;
		top: 0;
		z-index: 10;
	}

	.detail-title-input {
		font-size: 1.5rem;
		font-weight: 800;
		color: var(--base-content);
		background: transparent;
		border: 1px solid transparent;
		width: 100%;
		border-radius: 0.5rem;
		padding: 0.25rem 0.5rem;
		margin-left: -0.5rem;
		transition: all 0.2s;
	}

	.detail-title-input:focus {
		background: white;
		border-color: var(--brand-500);
		outline: none;
		box-shadow: 0 0 0 3px color-mix(in oklab, var(--brand-500) 15%, transparent);
	}

	.detail-meta {
		font-size: 0.85rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
		margin-top: 0.25rem;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.detail-content {
		padding: 1.5rem;
		display: grid;
		grid-template-columns: 1fr;
		gap: 2rem;
	}

	.detail-section {
		margin-bottom: 2rem;
	}

	.section-label {
		font-size: 0.85rem;
		font-weight: 700;
		color: color-mix(in oklab, var(--base-content) 70%, transparent);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: 0.75rem;
	}

	.detail-badges-row {
		display: flex;
		flex-wrap: wrap;
		gap: 2rem;
		margin-bottom: 1rem;
	}

	.description-editor {
		background: color-mix(in oklab, var(--rs-surface-muted) 30%, white);
		border-radius: 0.75rem;
		border: 1px solid var(--base-200);
		padding: 0.5rem;
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.75rem;
		background: var(--base-100);
		border: 1px solid var(--base-200);
		border-radius: 0.75rem;
		transition: all 0.2s;
	}

	.attachment-item:hover {
		background: white;
		border-color: var(--brand-500);
		cursor: pointer;
	}

	.attachment-icon {
		width: 2.5rem;
		height: 2.5rem;
		background: var(--base-200);
		border-radius: 0.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--base-content);
	}

	.attachment-name {
		font-weight: 700;
		font-size: 0.9rem;
	}

	.attachment-meta {
		font-size: 0.75rem;
		color: var(--base-content);
		opacity: 0.6;
	}

	.activity-feed {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.activity-item {
		display: flex;
		gap: 1rem;
	}

	.activity-avatar {
		width: 2rem;
		height: 2rem;
		border-radius: 999px;
		background: var(--brand-500);
		color: white;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 800;
		font-size: 0.8rem;
		flex-shrink: 0;
	}

	.activity-header {
		font-size: 0.85rem;
	}

	.activity-text {
		font-size: 0.9rem;
		color: var(--base-content);
		margin-top: 0.15rem;
	}

	.btn-close {
		padding: 0.5rem;
		border-radius: 999px;
		color: var(--base-content);
		opacity: 0.5;
		transition: all 0.2s;
	}

	.btn-close:hover {
		background: var(--base-200);
		opacity: 1;
	}

	@media (max-width: 767px) {
		.detail-badges-row {
			gap: 1rem;
		}
	}
</style>
