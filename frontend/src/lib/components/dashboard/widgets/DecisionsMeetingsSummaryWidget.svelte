<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { formatDistanceToNow } from '$lib/utils/format';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';

	let {
		module,
		modules = []
	}: {
		module: ModuleDefinition;
		modules?: ModuleDefinition[];
	} = $props();

	let widget = $derived(module.ui.dashboard.widget);
	let decisionsModule = $derived(modules.find((candidate) => candidate.key === 'decisions'));

	const meetingsSummaryQuery = createQuery({
		queryKey: ['module-summary', module.key],
		queryFn: () => getModuleSummary(module.key)
	});
	const decisionsSummaryQuery = createQuery({
		queryKey: ['module-summary', 'decisions'],
		queryFn: () => getModuleSummary('decisions'),
		enabled: Boolean(decisionsModule)
	});
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.key}`}>
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
			{#if (filterUserVisibleEntries($decisionsSummaryQuery.data?.recent_items ?? []).length ?? 0) === 0}
				<p class="empty-copy">No decisions recorded yet.</p>
			{:else}
				<ul>
					{#each filterUserVisibleEntries($decisionsSummaryQuery.data?.recent_items ?? []).slice(0, 2) as item}
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
			{#if (filterUserVisibleEntries($meetingsSummaryQuery.data?.recent_items ?? []).length ?? 0) === 0}
				<p class="empty-copy">No meeting notes yet.</p>
			{:else}
				<ul>
					{#each filterUserVisibleEntries($meetingsSummaryQuery.data?.recent_items ?? []).slice(0, 3) as item}
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
		gap: 0.6rem;
	}

	.streams section {
		padding-left: 0.6rem;
		border-left: 2px solid color-mix(in oklab, var(--base-300) 65%, transparent);
	}

	.streams header {
		margin-bottom: 0.4rem;
		font-size: 0.75rem;
		font-weight: 800;
	}

	.streams ul {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}

	.streams li {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
	}

	.streams strong {
		font-size: 0.8rem;
	}

	.streams span,
	.empty-copy {
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
