<script lang="ts">
	import type { ModuleDefinition } from '$lib/modules/registry';
	import DashboardWidgetRenderer from './DashboardWidgetRenderer.svelte';

	export let modules: ModuleDefinition[] = [];

	function columnClass(module: ModuleDefinition): string {
		const columns = module.ui.dashboard.widget.columns;
		return `desk-${columns.desktop} tab-${columns.tablet} mob-${columns.mobile}`;
	}
</script>

<div class="widget-grid" aria-label="Workspace dashboard widgets">
	{#each modules as module (module.key)}
		<div class={`widget-slot ${columnClass(module)}`}>
			<DashboardWidgetRenderer {module} {modules} />
		</div>
	{/each}
</div>

<style>
	.widget-grid {
		display: grid;
		grid-template-columns: repeat(12, minmax(0, 1fr));
		gap: 0.875rem;
		align-items: start;
	}

	.widget-slot {
		min-width: 0;
	}

	/* Force 4-column layout on desktop regardless of module config */
	.widget-slot.desk-3,
	.widget-slot.desk-4,
	.widget-slot.desk-5,
	.widget-slot.desk-6,
	.widget-slot.desk-12 {
		grid-column: span 3;
	}

	@media (max-width: 1199px) {
		.widget-grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.widget-slot[class*='tab-6'],
		.widget-slot[class*='tab-12'],
		.widget-slot {
			grid-column: span 1;
		}

		.widget-slot.tab-12 {
			grid-column: 1 / -1;
		}
	}

	@media (max-width: 767px) {
		.widget-grid {
			grid-template-columns: 1fr;
			gap: 0.75rem;
		}

		.widget-slot {
			grid-column: 1 / -1;
		}
	}
</style>
