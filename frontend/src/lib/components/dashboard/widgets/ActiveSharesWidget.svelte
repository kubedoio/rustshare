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
	$: extra = ($summaryQuery.data?.extra ?? {}) as { publicCount?: number; internalCount?: number };
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.module_key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{module.root_path}</span>
	</div>

	<div class="share-stats">
		<div>
			<strong>{extra.internalCount ?? 0}</strong>
			<span>Internal</span>
		</div>
		<div>
			<strong>{extra.publicCount ?? 0}</strong>
			<span>Public</span>
		</div>
	</div>

	<ul class="share-list">
		{#each $summaryQuery.data?.recent_items.slice(0, widget.maxItems) ?? [] as item}
			<li>{item.name}</li>
		{/each}
	</ul>
</a>

<style>
	@import './widgetStyles.css';

	.share-stats {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.8rem;
	}

	.share-stats div {
		padding: 0.85rem 0.95rem;
		border-radius: 1rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
	}

	.share-stats strong {
		display: block;
		font-size: 1rem;
		margin-bottom: 0.15rem;
	}

	.share-stats span,
	.share-list {
		font-size: 0.82rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}

	.share-list {
		margin: 0;
		padding-left: 1.1rem;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
</style>
