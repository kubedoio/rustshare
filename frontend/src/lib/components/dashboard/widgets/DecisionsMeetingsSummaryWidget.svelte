<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleConfig } from '$lib/api/types';
	import { getModuleDashboardWidgetConfig } from '$lib/modules/workspaceSurface';
	import { formatDistanceToNow } from 'date-fns';

	export let module: ModuleConfig;
	export let modules: ModuleConfig[] = [];

	$: widget = getModuleDashboardWidgetConfig(module);
	$: decisionsModule = modules.find((candidate) => candidate.module_key === 'decisions');
	$: meetingsSummaryQuery = createQuery({
		queryKey: ['module-summary', module.module_key],
		queryFn: () => getModuleSummary(module.module_key)
	});
	$: decisionsSummaryQuery = createQuery({
		queryKey: ['module-summary', 'decisions'],
		queryFn: () => getModuleSummary('decisions'),
		enabled: Boolean(decisionsModule)
	});
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.module_key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">Meetings + Decisions</span>
	</div>

	<div class="streams">
		<section>
			<header>Decisions</header>
			{#if ($decisionsSummaryQuery.data?.recent_items.length ?? 0) === 0}
				<p class="empty-copy">No decisions recorded yet.</p>
			{:else}
				<ul>
					{#each $decisionsSummaryQuery.data?.recent_items.slice(0, 2) ?? [] as item}
						<li>
							<strong>{item.name}</strong>
							<span>{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<section>
			<header>Meetings</header>
			{#if ($meetingsSummaryQuery.data?.recent_items.length ?? 0) === 0}
				<p class="empty-copy">No meeting notes yet.</p>
			{:else}
				<ul>
					{#each $meetingsSummaryQuery.data?.recent_items.slice(0, 3) ?? [] as item}
						<li>
							<strong>{item.name}</strong>
							<span>{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
</a>

<style>
	@import './widgetStyles.css';

	.streams {
		display: grid;
		grid-template-columns: 1fr;
		gap: 1rem;
	}

	.streams section {
		padding-left: 1rem;
		border-left: 2px solid color-mix(in oklab, var(--base-300) 65%, transparent);
	}

	.streams header {
		margin-bottom: 0.75rem;
		font-size: 0.9rem;
		font-weight: 800;
	}

	.streams ul {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.85rem;
	}

	.streams li {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
	}

	.streams strong {
		font-size: 0.94rem;
	}

	.streams span,
	.empty-copy {
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
