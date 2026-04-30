<script lang="ts">
	import type { ModuleConfig, WorkspaceSurfaceSection } from '$lib/api/types';
	import DashboardWidgetGrid from './DashboardWidgetGrid.svelte';

	export let section: WorkspaceSurfaceSection;
	export let modules: ModuleConfig[] = [];
</script>

<section class="insights-section" aria-labelledby="workspace-summary-insights">
	<div class="insights-header">
		<h2 id="workspace-summary-insights">{section.title ?? 'Workspace Summary & Insights'}</h2>
		<p>Registry-driven widgets pulled from your enabled workspace modules.</p>
	</div>

	<DashboardWidgetGrid {modules} />
</section>

<style>
	.insights-section {
		display: flex;
		flex-direction: column;
		gap: 1.35rem;
	}

	.insights-header h2 {
		margin: 0 0 0.35rem;
		font-size: clamp(1.7rem, 2.6vw, 2.2rem);
		font-weight: 800;
		letter-spacing: -0.02em;
	}

	.insights-header p {
		margin: 0;
		font-size: 0.95rem;
		color: color-mix(in oklab, var(--base-content) 62%, transparent);
	}
</style>
