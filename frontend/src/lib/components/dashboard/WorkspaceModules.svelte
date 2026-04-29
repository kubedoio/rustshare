<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import ModuleCard from './ModuleCard.svelte';
	import { LayoutGrid } from 'lucide-svelte';

	export let modules: ModuleConfig[];

	$: sortedModules = modules
		.filter((m) => m.ui_config?.dashboard?.enabled !== false)
		.sort((a, b) => (a.ui_config?.dashboard?.order ?? 99) - (b.ui_config?.dashboard?.order ?? 99));
</script>

<section class="modules-panel">
	<div class="modules-panel-header">
		<div class="modules-panel-title-row">
			<LayoutGrid size={16} class="text-brand-500" />
			<h2 class="modules-panel-title">Workspace Modules</h2>
		</div>
		<p class="modules-panel-subtitle">Enabled file-backed work areas in this workspace.</p>
	</div>

	{#if sortedModules.length === 0}
		<div class="modules-empty">
			<p class="text-sm text-base-content/50">
				No modules enabled. Ask an admin to enable modules in the Admin Dashboard.
			</p>
		</div>
	{:else}
		<div class="modules-grid">
			{#each sortedModules as module (module.id)}
				<ModuleCard {module} />
			{/each}
		</div>
	{/if}
</section>

<style>
	.modules-panel {
		background: var(--base-100);
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		border-radius: 1.5rem;
		padding: 1.25rem;
		box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
	}

	@media (min-width: 640px) {
		.modules-panel {
			padding: 1.5rem;
		}
	}

	.modules-panel-header {
		margin-bottom: 1rem;
	}

	.modules-panel-title-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.25rem;
	}

	.modules-panel-title {
		font-size: 0.875rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--base-content);
	}

	.modules-panel-subtitle {
		font-size: 0.75rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.modules-grid {
		display: grid;
		grid-template-columns: repeat(1, 1fr);
		gap: 0.75rem;
	}

	@media (min-width: 640px) {
		.modules-grid {
			grid-template-columns: repeat(2, 1fr);
			gap: 1rem;
		}
	}

	@media (min-width: 1024px) {
		.modules-grid {
			grid-template-columns: repeat(3, 1fr);
		}
	}

	.modules-empty {
		display: flex;
		justify-content: center;
		padding: 2rem 0;
	}
</style>
