<script lang="ts">
	import type { KanbanBoard, KanbanCard } from '$lib/api/types';
	import KanbanColumn from './KanbanColumn.svelte';

	interface Props {
		board: KanbanBoard;
		onCardClick: (card: KanbanCard) => void;
		onShowAddCard: (columnId: string) => void;
		onAddCard: (columnId: string, title: string) => void;
		onCancelAddCard: () => void;
		onNewCardTitleChange: (title: string) => void;
		onDragStart: (e: DragEvent, card: KanbanCard, columnId: string) => void;
		onDragEnd: () => void;
		onDragOver: (e: DragEvent, columnId: string) => void;
		onDragLeave: (columnId: string) => void;
		onDrop: (e: DragEvent, columnId: string) => void;
		draggingOverColumnId: string | null;
		showCreateCardColumnId: string | null;
		newCardTitle: string;
		setColumnRef: (columnId: string, el: HTMLDivElement | null) => void;
	}

	let {
		board,
		onCardClick,
		onShowAddCard,
		onAddCard,
		onCancelAddCard,
		onNewCardTitleChange,
		onDragStart,
		onDragEnd,
		onDragOver,
		onDragLeave,
		onDrop,
		draggingOverColumnId,
		showCreateCardColumnId,
		newCardTitle,
		setColumnRef
	}: Props = $props();
</script>

<div class="kanban-board-surface">
	{#each board.columns as column}
		<KanbanColumn
			{column}
			cards={column.cards}
			onCardClick={(card) => onCardClick(card)}
			onShowAddCard={() => onShowAddCard(column.id)}
			onAddCard={(title) => onAddCard(column.id, title)}
			{onCancelAddCard}
			{onNewCardTitleChange}
			onDragStart={(e, card) => onDragStart(e, card, column.id)}
			{onDragEnd}
			onDragOver={(e) => onDragOver(e, column.id)}
			onDragLeave={() => onDragLeave(column.id)}
			onDrop={(e) => onDrop(e, column.id)}
			isDragOver={draggingOverColumnId === column.id}
			isCreatingCard={showCreateCardColumnId === column.id}
			{newCardTitle}
			setContainerRef={(el) => setColumnRef(column.id, el)}
		/>
	{/each}
</div>

<style>
	.kanban-board-surface {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(16rem, 1fr);
		gap: 1rem;
		overflow-x: auto;
		padding-bottom: 0.75rem;
	}
</style>
