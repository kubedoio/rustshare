<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import { listKanbanBoards, getKanbanBoard } from '$lib/api/kanban';
	import type { ModuleDefinition } from '$lib/modules/registry';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const widget = $derived(module.ui.dashboard.widget);

	const summaryQuery = createQuery({
		queryKey: ['module-summary', module.key],
		queryFn: () => getModuleSummary(module.key)
	});

	const boardsQuery = $derived(
		createQuery({
			queryKey: ['kanban-boards-widget', module.key],
			queryFn: () => listKanbanBoards(1),
			enabled: !!$summaryQuery.data
		})
	);

	const latestBoardId = $derived($boardsQuery.data?.[0]?.id ?? '');

	const boardQuery = $derived(
		createQuery({
			queryKey: ['kanban-board-widget', latestBoardId],
			queryFn: () => getKanbanBoard(latestBoardId),
			enabled: !!latestBoardId
		})
	);

	const extra = $derived(
		($summaryQuery.data?.extra ?? {}) as { boards?: Array<{ id: string; name: string }> }
	);
	const maxItems = $derived(widget.maxItems ?? 8);
	const latestBoard = $derived($boardQuery.data);
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{extra.boards?.length ?? 0} boards</span>
	</div>

	<div class="kanban-scroll" aria-label="Kanban summary preview">
		{#if latestBoard}
			{#each latestBoard.columns as column}
				<section class="kanban-column">
					<header>{column.title}</header>
					{#each column.cards.slice(0, Math.max(1, Math.floor(maxItems / latestBoard.columns.length))) as card}
						<article class="kanban-card">
							<strong>{card.title}</strong>
							<p>{column.title}</p>
						</article>
					{/each}
					{#if column.cards.length === 0}
						<div class="kanban-empty">No cards</div>
					{/if}
				</section>
			{/each}
		{:else}
			<div class="kanban-empty">No boards yet. Create your first file-backed board.</div>
		{/if}
	</div>
</a>

<style>
	@import './widgetStyles.css';

	.kanban-scroll {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(10rem, 12rem);
		gap: 0.75rem;
		overflow-x: auto;
		padding-bottom: 0.5rem;
	}

	.kanban-column {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		min-height: 10rem;
		padding: 0.75rem;
		border-radius: 1rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 55%, white);
	}

	.kanban-column header {
		font-weight: 800;
		font-size: 0.85rem;
		color: var(--base-content);
	}

	.kanban-card {
		padding: 0.6rem 0.7rem;
		border-radius: 0.75rem;
		background: rgba(255, 255, 255, 0.78);
		border: 1px solid rgba(128, 93, 46, 0.08);
	}

	.kanban-card strong {
		display: block;
		margin-bottom: 0.2rem;
		font-size: 0.82rem;
		color: var(--base-content);
	}

	.kanban-card p,
	.kanban-empty {
		margin: 0;
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
	}

	.kanban-empty {
		display: flex;
		align-items: center;
		padding: 0.75rem;
		border-radius: 0.75rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 45%, white);
	}
</style>
