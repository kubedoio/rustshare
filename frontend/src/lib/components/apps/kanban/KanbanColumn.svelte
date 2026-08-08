<script lang="ts">
	import { Plus } from 'lucide-svelte';
	import type { KanbanColumn as ColumnType, KanbanCard as CardType } from '$lib/api/types';
	import KanbanCard from './KanbanCard.svelte';

	interface Props {
		column: ColumnType;
		cards: CardType[];
		onCardClick: (card: CardType) => void;
		onShowAddCard: () => void;
		onAddCard: (title: string) => void;
		onCancelAddCard: () => void;
		onNewCardTitleChange: (title: string) => void;
		onDragStart: (e: DragEvent, card: CardType) => void;
		onDragEnd: () => void;
		onDragOver: (e: DragEvent) => void;
		onDragLeave: () => void;
		onDrop: (e: DragEvent) => void;
		isDragOver: boolean;
		isCreatingCard: boolean;
		newCardTitle: string;
		setContainerRef: (el: HTMLDivElement | null) => void;
	}

	let {
		column,
		cards,
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
		isDragOver,
		isCreatingCard,
		newCardTitle,
		setContainerRef
	}: Props = $props();

	let containerEl: HTMLDivElement | null = $state(null);

	$effect(() => {
		setContainerRef(containerEl);
	});

	function formatColumnName(value: string): string {
		return value.replace(/^\d+-/, '').replace(/-/g, ' ');
	}
</script>

<section
	class="kanban-column"
	class:kanban-column-dragover={isDragOver}
	ondragover={onDragOver}
	ondragleave={onDragLeave}
	ondrop={onDrop}
	aria-label="{formatColumnName(column.title)} column"
>
	<header class="kanban-column-header">
		<h4>{formatColumnName(column.title)}</h4>
		<div class="flex items-center gap-2">
			<span>{cards.length}</span>
			<button
				class="text-base-content/40 hover:text-base-content"
				aria-label="Add card to {formatColumnName(column.title)}"
				onclick={onShowAddCard}
			>
				<Plus size={14} />
			</button>
		</div>
	</header>

	<div class="kanban-card-list" bind:this={containerEl}>
		{#if cards.length === 0}
			<div class="kanban-empty-column">
				No cards yet
				<button class="mt-1 block text-xs text-brand-500 hover:underline" onclick={onShowAddCard}>
					Add card
				</button>
			</div>
		{/if}

		{#each cards as card (card.id)}
			<KanbanCard
				{card}
				onClick={() => onCardClick(card)}
				onDragStart={(e) => onDragStart(e, card)}
				{onDragEnd}
			/>
		{/each}
	</div>

	<div class="mt-2">
		{#if isCreatingCard}
			<div class="flex flex-col gap-2 rounded-xl border border-base-300/60 bg-base-100 p-2">
				<input
					type="text"
					placeholder="Card title"
					class="input-bordered input input-sm w-full"
					value={newCardTitle}
					oninput={(e) => onNewCardTitleChange(e.currentTarget.value)}
					onkeydown={(e) => {
						if (e.key === 'Enter') onAddCard(newCardTitle);
						if (e.key === 'Escape') onCancelAddCard();
					}}
				/>
				<div class="flex gap-2">
					<button class="btn flex-1 btn-xs btn-primary" onclick={() => onAddCard(newCardTitle)}>
						Add
					</button>
					<button class="btn btn-ghost btn-xs" onclick={onCancelAddCard}> Cancel </button>
				</div>
			</div>
		{:else}
			<button class="btn w-full text-base-content/60 btn-ghost btn-xs" onclick={onShowAddCard}>
				<Plus size={12} />
				Add card
			</button>
		{/if}
	</div>
</section>

<style>
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

	.kanban-empty-column {
		border: 1px dashed color-mix(in oklab, var(--base-300) 60%, transparent);
		border-radius: 1rem;
		background: rgba(255, 255, 255, 0.45);
		padding: 1rem;
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}
</style>
