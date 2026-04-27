<script lang="ts">
	import { QueryClientProvider } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<QueryClientProvider client={queryClient}>
	{@render children()}
</QueryClientProvider>
