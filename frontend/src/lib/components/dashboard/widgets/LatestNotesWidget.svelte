<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { formatDistanceToNow } from '$lib/utils/format';
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

	{#if (filterUserVisibleEntries($summaryQuery.data?.recent_items ?? []).length ?? 0) === 0}
		<p class="empty-copy">No notes yet.</p>
	{:else}
		<ul class="note-list">
			{#each filterUserVisibleEntries($summaryQuery.data?.recent_items ?? []).slice(0, widget.maxItems) as note}
				<li>
					<div>
						<strong>{note.name}</strong>
						<p>Updated {formatDistanceToNow(new Date(note.updated_at), { addSuffix: true })}</p>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</a>

<style>
	@import './widgetStyles.css';

	.note-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}

	.note-list li {
		padding: 0.5rem 0.6rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
	}

	.note-list strong {
		display: block;
		font-size: 0.8rem;
		margin-bottom: 0.1rem;
	}

	.note-list p,
	.empty-copy {
		margin: 0;
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
