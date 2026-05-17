<script lang="ts">
	import { dashboardConfig } from '$lib/stores/dashboardConfig';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { Eye, EyeOff, ArrowUp, ArrowDown, RotateCcw, Check } from 'lucide-svelte';

	let {
		modules = []
	}: {
		modules?: ModuleDefinition[];
	} = $props();

	let config = $derived($dashboardConfig);
	let orderedModules = $derived(modules.slice().sort((a, b) => {
		const idxA = config.moduleOrder.indexOf(a.key);
		const idxB = config.moduleOrder.indexOf(b.key);
		return (idxA === -1 ? 999 : idxA) - (idxB === -1 ? 999 : idxB);
	}));

	function isEnabled(key: string): boolean {
		return config.enabledModules.includes(key);
	}

	function canMoveUp(key: string): boolean {
		const idx = config.moduleOrder.indexOf(key);
		return idx > 0;
	}

	function canMoveDown(key: string): boolean {
		const idx = config.moduleOrder.indexOf(key);
		return idx >= 0 && idx < config.moduleOrder.length - 1;
	}
</script>

<div class="settings-panel" role="region" aria-label="Dashboard settings">
	<div class="settings-header">
		<span class="settings-title">Customize dashboard</span>
		<div class="settings-actions">
			<button
				type="button"
				class="settings-btn reset"
				on:click={() => dashboardConfig.reset(modules)}
				title="Reset to defaults"
			>
				<RotateCcw size={14} />
				<span>Reset</span>
			</button>
			<button
				type="button"
				class="settings-btn done"
				on:click={() => dashboardConfig.setEditMode(false)}
			>
				<Check size={14} />
				<span>Done</span>
			</button>
		</div>
	</div>

	<ul class="module-list">
		{#each orderedModules as module (module.key)}
			<li class="module-row">
				<button
					type="button"
					class="toggle-btn"
					class:enabled={isEnabled(module.key)}
					on:click={() => dashboardConfig.toggleModule(module.key)}
					title={isEnabled(module.key) ? 'Hide module' : 'Show module'}
					aria-pressed={isEnabled(module.key)}
				>
					{#if isEnabled(module.key)}
						<Eye size={14} />
					{:else}
						<EyeOff size={14} />
					{/if}
				</button>

				<span class="module-name" class:dimmed={!isEnabled(module.key)}>
					{module.displayName}
				</span>

				<div class="reorder-btns">
					<button
						type="button"
						class="reorder-btn"
						disabled={!canMoveUp(module.key)}
						on:click={() => dashboardConfig.moveModule(module.key, 'up')}
						title="Move up"
					>
						<ArrowUp size={14} />
					</button>
					<button
						type="button"
						class="reorder-btn"
						disabled={!canMoveDown(module.key)}
						on:click={() => dashboardConfig.moveModule(module.key, 'down')}
						title="Move down"
					>
						<ArrowDown size={14} />
					</button>
				</div>
			</li>
		{/each}
	</ul>
</div>

<style>
	.settings-panel {
		padding: 1rem 1.25rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 45%, transparent);
		border-radius: 1.25rem;
		background: color-mix(in oklab, var(--base-100) 96%, white);
		animation: slideDown 200ms ease;
	}

	@keyframes slideDown {
		from {
			opacity: 0;
			transform: translateY(-6px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.settings-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.75rem;
	}

	.settings-title {
		font-size: 0.85rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.settings-actions {
		display: flex;
		gap: 0.5rem;
	}

	.settings-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.35rem 0.7rem;
		border-radius: 0.6rem;
		font-size: 0.78rem;
		font-weight: 700;
		border: 1px solid transparent;
		cursor: pointer;
		background: transparent;
		color: var(--base-content);
		transition: background 150ms ease;
	}

	.settings-btn:hover {
		background: color-mix(in oklab, var(--base-300) 18%, transparent);
	}

	.settings-btn.done {
		background: var(--brand-500);
		color: white;
	}

	.settings-btn.done:hover {
		background: var(--brand-600);
	}

	.module-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.module-row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.45rem 0.5rem;
		border-radius: 0.75rem;
		transition: background 120ms ease;
	}

	.module-row:hover {
		background: color-mix(in oklab, var(--base-300) 10%, transparent);
	}

	.toggle-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		background: transparent;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		cursor: pointer;
		transition:
			background 150ms ease,
			color 150ms ease,
			border-color 150ms ease;
	}

	.toggle-btn.enabled {
		background: var(--brand-500);
		border-color: var(--brand-500);
		color: white;
	}

	.toggle-btn:hover {
		border-color: var(--brand-500);
		color: var(--brand-500);
	}

	.toggle-btn.enabled:hover {
		background: var(--brand-600);
		color: white;
	}

	.module-name {
		flex: 1;
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--base-content);
		transition: opacity 150ms ease;
	}

	.module-name.dimmed {
		opacity: 0.5;
	}

	.reorder-btns {
		display: flex;
		gap: 0.25rem;
	}

	.reorder-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.6rem;
		height: 1.6rem;
		border-radius: 0.4rem;
		border: 1px solid transparent;
		background: transparent;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
		cursor: pointer;
		transition: background 150ms ease;
	}

	.reorder-btn:hover:not(:disabled) {
		background: color-mix(in oklab, var(--base-300) 18%, transparent);
		color: var(--base-content);
	}

	.reorder-btn:disabled {
		opacity: 0.25;
		cursor: not-allowed;
	}
</style>
