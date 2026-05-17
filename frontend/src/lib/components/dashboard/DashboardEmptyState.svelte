<script lang="ts">
	let {
		title = undefined,
		description,
		minimal = false
	}: {
		title?: string | undefined;
		description: string;
		minimal?: boolean;
	} = $props();
</script>

<div class="empty-state" class:minimal>
	{#if title}
		<p class="empty-title">{title}</p>
	{/if}
	<p class="empty-description">{description}</p>
</div>

<style>
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 2.5rem 1.5rem;
		text-align: center;
		border-radius: 1rem;
		border: 2px dashed color-mix(in oklab, var(--base-300) 50%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		min-height: 120px;
	}
	.empty-state.minimal {
		padding: 1.5rem;
	}
	.empty-title {
		margin: 0 0 0.35rem;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--base-content);
	}
	.empty-description {
		margin: 0;
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		max-width: 28rem;
		line-height: 1.5;
	}
</style>
