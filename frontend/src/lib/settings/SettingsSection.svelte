<script lang="ts">
	export let title: string;
	export let description: string | undefined = undefined;
</script>

<section class="py-6 first:pt-0 last:pb-0">
	<div class="mb-4">
		<h3 class="text-base font-semibold text-base-content">{title}</h3>
		{#if description}
			<p class="text-sm text-base-content/60 mt-1">{description}</p>
		{/if}
	</div>
	<div class="space-y-0 divide-y divide-base-300">
		<slot />
	</div>
</section>
