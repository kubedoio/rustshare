<script lang="ts">
	import { AlertCircle, RefreshCw } from 'lucide-svelte';

	interface Props {
		title?: string;
		message?: string;
		onRetry?: () => void;
		retryLabel?: string;
	}

	let {
		title = 'Something went wrong',
		message = 'Failed to load data. Please check your connection and try again.',
		onRetry,
		retryLabel = 'Try again'
	}: Props = $props();
</script>

<div class="flex h-full min-h-[16rem] flex-col items-center justify-center px-4 py-12 text-center">
	<div class="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-error/10">
		<AlertCircle size={28} class="text-error" />
	</div>
	<h3 class="mb-1 text-base font-semibold text-base-content">{title}</h3>
	<p class="mb-5 max-w-sm text-sm text-base-content/60">{message}</p>
	{#if onRetry}
		<button
			type="button"
			class="inline-flex items-center gap-2 rounded-xl bg-brand-500 px-4 py-2.5 text-sm font-medium text-white transition-colors hover:bg-brand-600"
			onclick={onRetry}
		>
			<RefreshCw size={16} />
			{retryLabel}
		</button>
	{/if}
</div>
