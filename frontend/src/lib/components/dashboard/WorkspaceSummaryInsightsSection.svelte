<script lang="ts">
	import type { WorkspaceSurfaceSection } from '$lib/modules/registry';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import DashboardWidgetGrid from './DashboardWidgetGrid.svelte';

	export let section: WorkspaceSurfaceSection;
	export let modules: ModuleDefinition[] = [];
</script>

<section class="insights-section" aria-labelledby="workspace-summary-insights">
	<div class="insights-header">
		<h2 id="workspace-summary-insights">{section.title ?? 'Workspace Summary & Insights'}</h2>
	</div>

	<DashboardWidgetGrid {modules} />
</section>

<style>
	.insights-section {
		display: flex;
		flex-direction: column;
	}

	.insights-header h2 {
		margin: 0 0 24px 0;
		font-size: clamp(1.7rem, 2.6vw, 2.2rem);
		font-weight: 800;
		letter-spacing: -0.02em;
	}
</style>
