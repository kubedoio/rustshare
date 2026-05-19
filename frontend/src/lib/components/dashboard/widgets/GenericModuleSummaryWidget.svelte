<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { ArrowRight, FileText, Folder } from 'lucide-svelte';
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
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{module.rootPath}</span>
	</div>

	{#if $summaryQuery.data}
		<p class="summary-count">
			{$summaryQuery.data.total_items} item{$summaryQuery.data.total_items === 1 ? '' : 's'}
		</p>
		<ul class="summary-list">
			{#each filterUserVisibleEntries($summaryQuery.data.recent_items).slice(0, widget.maxItems) as item}
				<li>
					{#if item.item_type === 'file'}
						<FileText size={11} />
					{:else}
						<Folder size={11} />
					{/if}
					<span>{item.name}</span>
				</li>
			{/each}
		</ul>
	{/if}

	<div class="widget-footer">
		<span>{widget.primaryAction?.label ?? 'Open Module'}</span>
		<ArrowRight size={12} />
	</div>
</a>

<style>
	@import './widgetStyles.css';

	.summary-count {
		margin: 0;
		font-size: 0.82rem;
		font-weight: 700;
	}

	.summary-list {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.summary-list li {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 72%, transparent);
	}

	.widget-footer {
		margin-top: auto;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-size: 0.78rem;
		font-weight: 700;
		color: var(--brand-500);
	}
</style>
