<script lang="ts">
	import { ChevronRight } from 'lucide-svelte';

	interface QuickAction {
		label: string;
		subtitle: string;
		icon: any;
		iconColor: string;
		iconBg: string;
		onClick: () => void;
	}

	export let actions: QuickAction[];
	export let creating: boolean = false;
</script>

<section class="quick-actions" aria-label="Quick actions">
	<h2 class="section-title">Quick actions</h2>
	<div class="action-list">
		{#each actions as action}
			{@const ActionIcon = action.icon}
			<button
				type="button"
				class="action-item"
				onclick={action.onClick}
				disabled={creating}
			>
				<div class="action-icon" style="background: {action.iconBg}; color: {action.iconColor};">
					<ActionIcon size={18} />
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
	.section-title {
		margin: 0 0 0.75rem;
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
	}
	.action-list {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.action-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.65rem 0.75rem;
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
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
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
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
		border-radius: 0.5rem;
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
