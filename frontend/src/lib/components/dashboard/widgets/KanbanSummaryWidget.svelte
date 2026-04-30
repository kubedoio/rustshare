<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleConfig } from '$lib/api/types';
	import { getModuleDashboardWidgetConfig } from '$lib/modules/workspaceSurface';

	export let module: ModuleConfig;

	$: widget = getModuleDashboardWidgetConfig(module);
	$: summaryQuery = createQuery({
		queryKey: ['module-summary', module.module_key],
		queryFn: () => getModuleSummary(module.module_key)
	});
	$: extra = ($summaryQuery.data?.extra ?? {}) as { boards?: Array<{ id: string; name: string }> };
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.module_key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{extra.boards?.length ?? 0} boards</span>
	</div>

	<div class="kanban-scroll" aria-label="Kanban summary preview">
		{#each extra.boards ?? [] as board, index}
			<section class={`kanban-column kanban-column-${index % 4}`}>
				<header>{board.name}</header>
				{#each ($summaryQuery.data?.recent_items ?? []).slice(index, index + 2) as card}
					<article class="kanban-card">
						<strong>{card.name}</strong>
						<p>{card.item_type === 'folder' ? 'Board card' : 'Linked file'}</p>
					</article>
				{/each}
			</section>
		{/each}

		{#if (extra.boards?.length ?? 0) === 0}
			<div class="kanban-empty">No boards yet. Create your first file-backed board.</div>
		{/if}
	</div>
</a>

<style>
	@import './widgetStyles.css';

	.kanban-scroll {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: minmax(14rem, 15rem);
		gap: 0.9rem;
		overflow-x: auto;
		padding-bottom: 0.5rem;
	}

	.kanban-column {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		min-height: 18rem;
		padding: 0.9rem;
		border-radius: 1.2rem;
	}

	.kanban-column-0 {
		background: color-mix(in oklab, #ffe3dd 78%, white);
	}
	.kanban-column-1 {
		background: color-mix(in oklab, #dfeeff 72%, white);
	}
	.kanban-column-2 {
		background: color-mix(in oklab, #dff5ef 78%, white);
	}
	.kanban-column-3 {
		background: color-mix(in oklab, #edf8d9 78%, white);
	}

	.kanban-column header {
		font-weight: 800;
		font-size: 1rem;
	}

	.kanban-card {
		padding: 0.85rem 0.9rem;
		border-radius: 1rem;
		background: rgba(255, 255, 255, 0.78);
		border: 1px solid rgba(128, 93, 46, 0.08);
	}

	.kanban-card strong {
		display: block;
		margin-bottom: 0.25rem;
		font-size: 0.9rem;
	}

	.kanban-card p,
	.kanban-empty {
		margin: 0;
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.kanban-empty {
		display: flex;
		align-items: center;
		padding: 1rem;
		border-radius: 1rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 65%, white);
	}
</style>
