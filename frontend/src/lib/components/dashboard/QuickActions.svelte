<script lang="ts">
	import { ChevronRight } from 'lucide-svelte';
	import ModuleIcon from './ModuleIcon.svelte';

	import type { Component } from 'svelte';

	interface QuickAction {
		label: string;
		subtitle: string;
		icon: unknown | string;
		iconColor: string;
		iconBg: string;
		onClick: () => void;
	}

	let {
		actions,
		creating = false
	}: {
		actions: QuickAction[];
		creating?: boolean;
	} = $props();
</script>

<section class="quick-actions" aria-label="Quick actions">
	<header class="section-header">
		<h2 class="section-title">Quick actions</h2>
	</header>
	<div class="action-list">
		{#each actions as action}
			<button type="button" class="action-item" onclick={action.onClick} disabled={creating}>
				<div class="action-icon" style="background: {action.iconBg}; color: {action.iconColor};">
					{#if typeof action.icon === 'string'}
						<ModuleIcon name={action.icon} size={18} />
					{:else}
						{@const ActionIcon = action.icon as Component}
						<ActionIcon size={18} />
					{/if}
				</div>
				<div class="action-body">
					<span class="action-label">{action.label}</span>
					<span class="action-subtitle">{action.subtitle}</span>
				</div>
				<ChevronRight size={16} class="action-chevron" />
			</button>
		{/each}
	</div>
</section>

<style>
	.quick-actions {
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--base-100) 94%, white);
		overflow: hidden;
	}
	.section-header {
		padding: 1rem 1rem 0.75rem;
	}
	.section-title {
		margin: 0;
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
	}
	.action-list {
		display: flex;
		flex-direction: column;
	}
	.action-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-height: 3.75rem;
		padding: 0.75rem 1rem;
		border: 0;
		border-top: 1px solid color-mix(in oklab, var(--base-300) 44%, transparent);
		background: transparent;
		color: inherit;
		font-size: inherit;
		font-family: inherit;
		cursor: pointer;
		transition:
			border-color 150ms ease,
			background 150ms ease;
		text-align: left;
		width: 100%;
	}
	.action-item:hover:not(:disabled) {
		background: color-mix(in oklab, var(--brand-500) 4%, var(--base-100));
	}
	.action-item:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--brand-500) 72%, transparent);
		outline-offset: -2px;
	}
	.action-item:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.action-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.45rem;
		flex-shrink: 0;
	}
	.action-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}
	.action-label {
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.action-subtitle {
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	:global(.action-chevron) {
		color: color-mix(in oklab, var(--base-content) 30%, transparent);
		flex-shrink: 0;
		transition: color 150ms ease;
	}
	.action-item:hover:not(:disabled) :global(.action-chevron) {
		color: var(--brand-500);
	}
</style>
