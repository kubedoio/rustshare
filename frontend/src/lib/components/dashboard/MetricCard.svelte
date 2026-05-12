<script lang="ts">
	export let label: string;
	export let value: string | number;
	export let subtitle: string;
	export let icon: any;
	export let iconColor: string;
	export let iconBg: string;
</script>

<div class="summary-card">
	<div class="summary-icon" style="background: {iconBg}; color: {iconColor};">
		<svelte:component this={icon} size={18} />
	</div>
	<div class="summary-body">
		<span class="summary-value">{value}</span>
		<span class="summary-label">{label}</span>
		<span class="summary-subtitle">{subtitle}</span>
	</div>
</div>

<style>
	.summary-card {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem 1.1rem;
		border-radius: 0.875rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 45%, transparent);
		background: color-mix(in oklab, var(--base-100) 94%, white);
		box-shadow: 0 4px 16px rgb(72 42 17 / 0.04);
		transition: border-color 150ms ease;
		min-width: 0;
	}
	.summary-card:hover {
		border-color: color-mix(in oklab, var(--brand-500) 25%, transparent);
	}
	.summary-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		border-radius: 0.65rem;
		flex-shrink: 0;
	}
	.summary-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		overflow: hidden;
	}
	.summary-value {
		font-size: 1.35rem;
		font-weight: 700;
		color: var(--base-content);
		line-height: 1.2;
	}
	.summary-label {
		font-size: 0.78rem;
		font-weight: 600;
		color: var(--base-content);
	}
	.summary-subtitle {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}
</style>
