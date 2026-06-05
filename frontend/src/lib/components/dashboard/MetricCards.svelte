<script lang="ts">
	import { ChevronRight } from 'lucide-svelte';

	interface Card {
		label: string;
		value: string | number;
		subtitle: string;
		icon: any;
		iconColor: string;
		iconBg: string;
		href?: string;
	}

	let {
		cards
	}: {
		cards: Card[];
	} = $props();
</script>

<section class="summary-cards" aria-label="Workspace summary">
	{#each cards as card}
		{#if card.href}
			<a href={card.href} class="summary-region">
				<div class="summary-icon" style="background: {card.iconBg}; color: {card.iconColor};">
					<svelte:component this={card.icon} size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{card.value}</span>
					<span class="summary-label">{card.label}</span>
					<span class="summary-subtitle">{card.subtitle}</span>
				</div>
				<ChevronRight size={16} class="summary-arrow" aria-hidden="true" />
			</a>
		{:else}
			<div class="summary-region">
				<div class="summary-icon" style="background: {card.iconBg}; color: {card.iconColor};">
					<svelte:component this={card.icon} size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{card.value}</span>
					<span class="summary-label">{card.label}</span>
					<span class="summary-subtitle">{card.subtitle}</span>
				</div>
			</div>
		{/if}
	{/each}
</section>

<style>
	.summary-cards {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		border: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--base-100) 94%, white);
		overflow: hidden;
	}

	.summary-region {
		display: flex;
		align-items: center;
		gap: 0.875rem;
		min-width: 0;
		min-height: 6rem;
		padding: 1rem 1.1rem;
		color: inherit;
		text-decoration: none;
		border-left: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		transition:
			background 150ms ease,
			outline-color 150ms ease;
	}

	.summary-region:first-child {
		border-left: 0;
	}

	a.summary-region:hover {
		background: color-mix(in oklab, var(--brand-500) 4%, var(--base-100));
	}

	a.summary-region:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--brand-500) 72%, transparent);
		outline-offset: -2px;
	}

	.summary-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 0.45rem;
		flex: 0 0 auto;
	}

	.summary-body {
		display: flex;
		flex-direction: column;
		gap: 0.12rem;
		min-width: 0;
		flex: 1;
	}

	.summary-value {
		font-size: 1.45rem;
		font-weight: 700;
		line-height: 1.1;
		color: var(--base-content);
	}

	.summary-label {
		font-size: 0.78rem;
		font-weight: 650;
		line-height: 1.3;
		color: var(--base-content);
	}

	.summary-subtitle {
		font-size: 0.72rem;
		line-height: 1.35;
		color: color-mix(in oklab, var(--base-content) 52%, transparent);
	}

	:global(.summary-arrow) {
		flex: 0 0 auto;
		color: color-mix(in oklab, var(--base-content) 36%, transparent);
	}

	@media (max-width: 767px) {
		.summary-cards {
			grid-template-columns: minmax(0, 1fr);
		}

		.summary-region {
			min-height: 5.5rem;
			border-left: 0;
			border-top: 1px solid color-mix(in oklab, var(--base-300) 52%, transparent);
		}

		.summary-region:first-child {
			border-top: 0;
		}
	}
</style>
