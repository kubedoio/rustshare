<script lang="ts">
	import type { Snippet } from 'svelte';
	import { onMount } from 'svelte';
	import { QueryClientProvider } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import favicon from '$lib/assets/favicon.svg';
	import { productBranding } from '$lib/branding';
	import './layout.css';
	let { children }: { children: Snippet } = $props();

	onMount(() => {
		document.documentElement.dataset.brandingTheme = productBranding.theme;
	});
</script>

<svelte:head>
	<title>{productBranding.productName}</title>
	<meta name="application-name" content={productBranding.productName} />
	<link rel="icon" href={productBranding.logoUrl ?? favicon} />
</svelte:head>
<QueryClientProvider client={queryClient}>
	{@render children()}
</QueryClientProvider>
