<script lang="ts">
	import MetricCard from './MetricCard.svelte';

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
		<MetricCard
			label={card.label}
			value={card.value}
			subtitle={card.subtitle}
			icon={card.icon}
			iconColor={card.iconColor}
			iconBg={card.iconBg}
			href={card.href}
		/>
	{/each}
</section>

<style>
	.summary-cards {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.875rem;
	}

	@media (max-width: 1023px) {
		.summary-cards {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
	}

	@media (max-width: 767px) {
		.summary-cards {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: 0.625rem;
		}
	}
</style>
