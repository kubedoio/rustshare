<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import DashboardWidgetGrid from './DashboardWidgetGrid.svelte';

	let {
		modules = []
	}: {
		modules?: ModuleDefinition[];
	} = $props();
</script>

<section class="insights-section">
	<DashboardWidgetGrid {modules} />
</section>

<style>
	.insights-section {
		display: flex;
		flex-direction: column;
	}
</style>
