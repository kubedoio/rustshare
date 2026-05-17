<script lang="ts">
	let {
		title,
		href = undefined,
		onClick = undefined
	}: {
		title: string;
		href?: string | undefined;
		onClick?: (() => void) | undefined;
	} = $props();
</script>

<div class="section-header">
	<h2 class="section-title">{title}</h2>
	{#if href}
		<a {href} class="view-all-link">View all</a>
	{:else if onClick}
		<button class="view-all-link" onclick={onClick}>View all</button>
	{/if}
</div>

<style>
	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.75rem;
	}
	.section-title {
		margin: 0;
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
	}
	.view-all-link {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--brand-500);
		text-decoration: none;
		background: none;
		border: none;
		cursor: pointer;
		padding: 0;
	}
	.view-all-link:hover {
		text-decoration: underline;
	}
</style>
