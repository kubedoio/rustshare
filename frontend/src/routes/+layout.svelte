<script lang="ts">
	import { QueryClientProvider } from '@tanstack/svelte-query';
	import { queryClient } from '$lib/query-client';
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<QueryClientProvider client={queryClient}>
	<slot />
</QueryClientProvider>
