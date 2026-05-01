<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { formatDistanceToNow } from 'date-fns';

	export let module: ModuleDefinition;

	$: widget = module.ui.dashboard.widget;
	$: summaryQuery = createQuery({
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

	{#if ($summaryQuery.data?.recent_items.length ?? 0) === 0}
		<p class="empty-copy">No standups yet.</p>
	{:else}
		<ul class="standup-list">
			{#each $summaryQuery.data?.recent_items.slice(0, widget.maxItems) ?? [] as item}
				<li>
					<div>
						<strong>{item.name}</strong>
						<p>{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}</p>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</a>

<style>
	@import './widgetStyles.css';

	.standup-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
	}

	.standup-list li {
		padding: 0.85rem 0.95rem;
		border-radius: 1rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
	}

	.standup-list strong {
		display: block;
		font-size: 0.94rem;
		margin-bottom: 0.2rem;
	}

	.standup-list p,
	.empty-copy {
		margin: 0;
		font-size: 0.82rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
