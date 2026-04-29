<script lang="ts">
	import { page } from '$app/stores';
	import { Home, AlertCircle } from 'lucide-svelte';
</script>

<div class="flex h-full flex-col items-center justify-center px-4 text-center">
	<div class="mb-6 flex h-16 w-16 items-center justify-center rounded-2xl bg-brand-500/10">
		<AlertCircle size={32} class="text-brand-500" />
	</div>

	<h1 class="text-title-xl mb-2 font-semibold text-base-content">
		{$page.status === 404 ? 'Page not found' : 'Something went wrong'}
	</h1>

	<p class="text-body-md mb-8 max-w-md text-base-content/60">
		{$page.status === 404
			? "The page you're looking for doesn't exist or has been moved."
			: $page.error?.message || 'An unexpected error occurred. Please try again later.'}
	</p>

	<div class="flex flex-wrap items-center justify-center gap-3">
		<a
			href="/files"
			class="inline-flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-600"
		>
			<Home size={18} />
			Go to Files
		</a>
		<a
			href="/dashboard"
			class="inline-flex items-center gap-2 rounded-lg border border-base-300 bg-base-100 px-4 py-2.5 text-sm font-medium text-base-content transition-colors hover:bg-base-200"
		>
			Go to Dashboard
		</a>
	</div>

	{#if $page.status !== 404}
		<p class="text-mono-sm mt-6 text-base-content/40">Error code: {$page.status}</p>
	{/if}
</div>
