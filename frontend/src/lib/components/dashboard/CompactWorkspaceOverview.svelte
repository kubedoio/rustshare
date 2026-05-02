<script lang="ts">
	import StorageCapacityRing from './StorageCapacityRing.svelte';

	export let workspaceTitle: string;
	export let storageQuota: number | null = null;
	export let storageUsed = 0;
</script>

<section class="overview-shell rs-surface" aria-label="Workspace overview">
	<div class="overview-title-block">
		<p class="overview-kicker">Workspace Dashboard</p>
		<h1>{workspaceTitle}</h1>
	</div>

	<StorageCapacityRing used={storageUsed} quota={storageQuota} />
</section>

<style>
	.overview-shell {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1.25rem;
		padding: 1.1rem 1.5rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 58%, transparent);
		border-radius: 1.75rem;
		min-height: 6rem;
	}

	.overview-title-block {
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 0.35rem;
		min-width: 0;
	}

	.overview-kicker {
		margin: 0;
		font-size: 0.68rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: color-mix(in oklab, var(--base-content) 58%, transparent);
	}

	.overview-title-block h1 {
		margin: 0;
		font-size: clamp(1.6rem, 2.5vw, 2.15rem);
		line-height: 1.08;
		font-weight: 700;
		color: var(--base-content);
		font-family: 'Fraunces', serif;
	}

	@media (max-width: 767px) {
		.overview-shell {
			flex-direction: column;
			align-items: flex-start;
			padding: 1.1rem;
			gap: 0.85rem;
		}

		.overview-title-block h1 {
			font-size: 1.5rem;
		}
	}
</style>
