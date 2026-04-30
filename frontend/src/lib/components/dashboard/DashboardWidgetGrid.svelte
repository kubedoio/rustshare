<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import { getModuleDashboardWidgetConfig } from '$lib/modules/workspaceSurface';
	import DashboardWidgetRenderer from './DashboardWidgetRenderer.svelte';

	export let modules: ModuleConfig[] = [];

	function columnClass(module: ModuleConfig): string {
		const columns = getModuleDashboardWidgetConfig(module).columns;
		return `desk-${columns.desktop} tab-${columns.tablet} mob-${columns.mobile}`;
	}
</script>

<div class="widget-grid" aria-label="Workspace dashboard widgets">
	{#each modules as module (module.module_key)}
		<div class={`widget-slot ${columnClass(module)}`}>
			<DashboardWidgetRenderer {module} {modules} />
		</div>
	{/each}
</div>

<style>
	.widget-grid {
		display: grid;
		grid-template-columns: repeat(12, minmax(0, 1fr));
		gap: 1.5rem;
		align-items: start;
	}

	.widget-slot {
		min-width: 0;
	}

	.widget-slot.desk-3 {
		grid-column: span 3;
	}
	.widget-slot.desk-4 {
		grid-column: span 4;
	}
	.widget-slot.desk-5 {
		grid-column: span 5;
	}
	.widget-slot.desk-6 {
		grid-column: span 6;
	}
	.widget-slot.desk-12 {
		grid-column: span 12;
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
			gap: 1rem;
		}

		.widget-slot {
			grid-column: 1 / -1;
		}
	}
</style>
