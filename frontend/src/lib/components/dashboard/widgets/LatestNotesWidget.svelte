<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import type { ModuleConfig } from '$lib/api/types';
	import { getModuleDashboardWidgetConfig } from '$lib/modules/workspaceSurface';
	import { formatDistanceToNow } from 'date-fns';

	export let module: ModuleConfig;

	$: widget = getModuleDashboardWidgetConfig(module);
	$: summaryQuery = createQuery({
		queryKey: ['module-summary', module.module_key],
		queryFn: () => getModuleSummary(module.module_key)
	});
</script>

<a class="widget-card widget-link" data-size={widget.size} href={`/modules/${module.module_key}`}>
	<div class="widget-header">
		<div>
			<h3>{widget.title}</h3>
			<p>{widget.description}</p>
		</div>
		<span class="widget-chip">{module.root_path}</span>
	</div>

	{#if ($summaryQuery.data?.recent_items.length ?? 0) === 0}
		<p class="empty-copy">No notes yet.</p>
	{:else}
		<ul class="note-list">
			{#each $summaryQuery.data?.recent_items.slice(0, widget.maxItems) ?? [] as note}
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
		gap: 0.8rem;
	}

	.note-list li {
		padding: 0.85rem 0.95rem;
		border-radius: 1rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 58%, white);
	}

	.note-list strong {
		display: block;
		font-size: 0.94rem;
		margin-bottom: 0.2rem;
	}

	.note-list p,
	.empty-copy {
		margin: 0;
		font-size: 0.82rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
