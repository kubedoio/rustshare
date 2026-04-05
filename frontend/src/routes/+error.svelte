<script lang="ts">
	import { page } from '$app/stores';
	import { Home, AlertCircle, ArrowLeft } from 'lucide-svelte';
</script>

<div class="min-h-screen bg-base-100 flex flex-col items-center justify-center px-4 text-center">
	<div class="mb-6 flex h-20 w-20 items-center justify-center rounded-2xl bg-brand-500/10">
		<AlertCircle size={40} class="text-brand-500" />
	</div>

	<h1 class="text-4xl font-display font-semibold text-base-content mb-2">
		{$page.status === 404 ? 'Page not found' : 'Something went wrong'}
	</h1>

	<p class="text-body-lg text-base-content/60 mb-8 max-w-md">
		{$page.status === 404
			? "The page you're looking for doesn't exist or has been moved."
			: ($page.error?.message || 'An unexpected error occurred. Please try again later.')}
	</p>

	<div class="flex flex-wrap items-center justify-center gap-3">
		<a
			href="/files"
			class="inline-flex items-center gap-2 rounded-lg bg-brand-500 px-5 py-2.5 text-sm font-medium text-white hover:bg-brand-600 transition-colors"
		>
			<Home size={18} />
			Go to Files
		</a>
		<a
			href="/dashboard"
			class="inline-flex items-center gap-2 rounded-lg border border-base-300 bg-base-100 px-5 py-2.5 text-sm font-medium text-base-content hover:bg-base-200 transition-colors"
		>
			<ArrowLeft size={18} />
			Back to Dashboard
		</a>
	</div>

	{#if $page.status !== 404}
		<p class="mt-8 text-mono-sm text-base-content/40">Error code: {$page.status}</p>
	{/if}

	<!-- Footer -->
	<div class="mt-16 text-sm text-base-content/40">
		<p>RustShare — Secure file sharing for teams</p>
	</div>
</div>
