<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';

	let {
		module
	}: {
		module: ModuleDefinition;
	} = $props();

	let widget = $derived(module.ui.dashboard.widget);
	const summaryQuery = createQuery({
		queryKey: ['module-summary', module.key],
		queryFn: () => getModuleSummary(module.key)
	});
	let extra = $derived(
		($summaryQuery.data?.extra ?? {}) as { publicCount?: number; internalCount?: number }
	);
	let visibleItems = $derived(filterUserVisibleEntries($summaryQuery.data?.recent_items ?? []));
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{module.rootPath}</span>
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
		{#each visibleItems.slice(0, widget.maxItems) as item}
			<li>{item.name}</li>
		{/each}
	</ul>
</a>

<style>
	@import './widgetStyles.css';

	.share-stats {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.5rem;
	}

	.share-stats div {
		padding: 0.5rem 0.6rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
	}

	.share-stats strong {
		display: block;
		font-size: 0.85rem;
		margin-bottom: 0.1rem;
	}

	.share-stats span,
	.share-list {
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}

	.share-list {
		margin: 0;
		padding-left: 0.9rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
</style>
