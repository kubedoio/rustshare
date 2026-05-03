<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import { listKanbanBoards, getKanbanBoard } from '$lib/api/kanban';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { Layout, Clock, Plus, ChevronRight, AlertCircle } from 'lucide-svelte';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const widget = $derived(module.ui.dashboard.widget);

	const summaryQuery = createQuery({
		get queryKey() {
			return ['module-summary', module.key];
		},
		queryFn: () => getModuleSummary(module.key)
	});

	const boardsQuery = createQuery({
		get queryKey() {
			return ['kanban-boards-widget', module.key];
		},
		queryFn: () => listKanbanBoards()
	});

	const latestBoardSummary = $derived.by(() => {
		const boards = $boardsQuery.data ?? [];
		if (boards.length === 0) return null;

		const activeBoards = boards.filter((b) => !b.archived);
		const targetBoards = activeBoards.length > 0 ? activeBoards : boards;

		return [...targetBoards].sort((a, b) => {
			const timeA = a.updated_at ? new Date(a.updated_at).getTime() : 0;
			const timeB = b.updated_at ? new Date(b.updated_at).getTime() : 0;
			return timeB - timeA;
		})[0];
	});

	const latestBoardId = $derived(latestBoardSummary?.id ?? '');

	const boardQuery = $derived(
		createQuery({
			queryKey: ['kanban-board-widget', latestBoardId],
			queryFn: () => getKanbanBoard(latestBoardId),
			enabled: !!latestBoardId
		})
	);

	const latestBoard = $derived($boardQuery.data);
	const maxItems = $derived(widget.maxItems ?? 10);

	function formatActivityDate(dateStr?: string) {
		if (!dateStr) return 'Unknown';
		const date = new Date(dateStr);
		if (isNaN(date.getTime())) return 'Unknown';
		const now = new Date();
		const diff = now.getTime() - date.getTime();

		if (diff < 60000) return 'Just now';
		if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
		if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;

		return new Intl.DateTimeFormat('en-US', {
			month: 'short',
			day: 'numeric'
		}).format(date);
	}
</script>

<div class="widget-card kanban-summary-widget" data-size={widget.size}>
	<div class="widget-header">
		<div class="flex-1">
			<a href={`/modules/${module.key}`} class="group no-underline">
				<h3
					class="m-0 flex items-center gap-2 text-base font-bold transition-colors group-hover:text-brand-500"
				>
					<Layout size={18} class="text-brand-500" />
					{widget.title}
				</h3>
			</a>
			{#if latestBoardSummary}
				<div
					class="mt-1 flex items-center gap-2 text-[10px] font-bold tracking-wider text-base-content/40 uppercase"
				>
					<span class="text-brand-500/80">{latestBoardSummary.title}</span>
					<span>•</span>
					<span>{latestBoardSummary.card_count} active cards</span>
					<span>•</span>
					<div class="flex items-center gap-1">
						<Clock size={10} />
						{formatActivityDate(latestBoardSummary.updated_at)}
					</div>
				</div>
			{:else}
				<p class="m-0 text-xs text-base-content/60">{widget.description}</p>
			{/if}
		</div>
		<a
			href={`/modules/${module.key}`}
			class="btn gap-1 rounded-full border border-base-200 px-2 btn-ghost btn-xs"
		>
			Open
			<ChevronRight size={12} />
		</a>
	</div>

	<div class="kanban-preview-grid" aria-label="Kanban summary preview">
		{#if $boardQuery.isPending && latestBoardId}
			<div class="kanban-loading">
				<span class="loading loading-md loading-spinner text-brand-500/20"></span>
			</div>
		{:else if $boardQuery.isError}
			<div class="kanban-empty error">
				<AlertCircle size={24} class="mb-2 text-error/40" />
				<p>Kanban summary unavailable.</p>
			</div>
		{:else if latestBoard}
			<div class="kanban-scroll">
				{#each latestBoard.columns as column}
					{@const maxColCards = Math.max(1, Math.floor(maxItems / latestBoard.columns.length))}
					{@const columnCards = column.cards.slice(0, maxColCards)}
					<section class="kanban-column">
						<header class="flex items-center justify-between">
							<span>{column.title.replace(/^\d+-/, '').replace(/-/g, ' ')}</span>
							<span class="text-[10px] opacity-40">{column.cards.length}</span>
						</header>
						<div class="flex flex-col gap-1.5">
							{#each columnCards as card}
								<a
									href={`/modules/${module.key}?boardId=${latestBoardId}&cardId=${card.id}`}
									class="kanban-mini-card group"
								>
									<span class="truncate">{card.title}</span>
									{#if card.priority && card.priority !== 'normal'}
										<div class="priority-dot priority-{card.priority}"></div>
									{/if}
								</a>
							{/each}
							{#if column.cards.length > maxColCards}
								<div class="text-center text-[10px] font-medium italic opacity-30">
									+{column.cards.length - maxColCards} more
								</div>
							{/if}
							{#if column.cards.length === 0}
								<div class="kanban-mini-empty">No active cards.</div>
							{/if}
						</div>
					</section>
				{/each}
			</div>
		{:else if !$boardsQuery.isPending}
			<div class="kanban-empty">
				<p>No Kanban boards yet. Create your first file-backed board.</p>
				<a
					href={`/modules/${module.key}`}
					class="btn mt-3 rounded-full btn-outline btn-xs btn-primary"
				>
					<Plus size={14} />
					Create Board
				</a>
			</div>
		{/if}
	</div>
</div>

<style>
	@import './widgetStyles.css';

	.kanban-summary-widget {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.widget-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		margin-bottom: 1.25rem;
	}

	.kanban-preview-grid {
		flex: 1;
		min-height: 10rem;
		display: flex;
		overflow: hidden;
	}

	.kanban-loading {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.kanban-scroll {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(8.5rem, 1fr);
		gap: 0.75rem;
		width: 100%;
		overflow-x: auto;
		padding-bottom: 0.75rem;
		/* Custom scrollbar for premium feel */
		scrollbar-width: thin;
		scrollbar-color: var(--base-300) transparent;
	}

	.kanban-scroll::-webkit-scrollbar {
		height: 4px;
	}
	.kanban-scroll::-webkit-scrollbar-thumb {
		background: var(--base-300);
		border-radius: 10px;
	}

	.kanban-column {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.75rem;
		border-radius: 1.25rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 35%, white);
		border: 1px solid color-mix(in oklab, var(--base-300) 30%, transparent);
	}

	.kanban-column header {
		font-weight: 800;
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
		margin-bottom: 0.25rem;
		padding: 0 0.25rem;
	}

	.kanban-mini-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		border-radius: 0.85rem;
		background: white;
		border: 1px solid rgba(0, 0, 0, 0.03);
		font-size: 0.75rem;
		font-weight: 700;
		color: var(--base-content);
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.02);
		text-decoration: none;
		transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.kanban-mini-card:hover {
		transform: translateY(-1.5px);
		border-color: var(--brand-500);
		box-shadow: 0 6px 15px rgba(0, 0, 0, 0.06);
		color: var(--brand-600);
	}

	.priority-dot {
		width: 6px;
		height: 6px;
		border-radius: 999px;
		flex-shrink: 0;
	}

	.priority-urgent {
		background: #eb5a46;
		box-shadow: 0 0 4px #eb5a46;
	}
	.priority-high {
		background: #ff9f1a;
	}
	.priority-low {
		background: #b3bac5;
	}

	.kanban-mini-empty {
		font-size: 0.65rem;
		text-align: center;
		padding: 0.75rem;
		opacity: 0.3;
		font-style: italic;
		font-weight: 500;
	}

	.kanban-empty {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 2rem;
		text-align: center;
		background: color-mix(in oklab, var(--rs-surface-muted) 25%, white);
		border-radius: 1.5rem;
		border: 2px dashed color-mix(in oklab, var(--base-300) 50%, transparent);
	}

	.kanban-empty p {
		font-size: 0.9rem;
		font-weight: 500;
		line-height: 1.5;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		max-width: 16rem;
		margin: 0;
	}

	.kanban-empty.error p {
		color: var(--error);
	}
</style>
