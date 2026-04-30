<script lang="ts">
	import { ChevronRight } from 'lucide-svelte';

	interface Props {
		label: string;
		description?: string;
		value?: string;
		actionLabel?: string;
		actionHref?: string;
		danger?: boolean;
		loading?: boolean;
		compact?: boolean;
		onAction?: () => void;
		content?: import('svelte').Snippet;
		action?: import('svelte').Snippet;
	}

	let {
		label,
		description = undefined,
		value = undefined,
		actionLabel = undefined,
		actionHref = undefined,
		danger = false,
		loading = false,
		compact = false,
		onAction = () => {},
		content,
		action
	}: Props = $props();

	function handleClick() {
		onAction();
	}
</script>

<div class="flex items-center justify-between gap-4 py-4 {compact ? 'py-3' : ''}">
	<div class="min-w-0 flex-1">
		<p class="text-sm font-medium text-base-content">{label}</p>
		{#if description}
			<p class="mt-0.5 text-sm text-base-content/60">{description}</p>
		{/if}
		{#if value}
			<p class="mt-1 truncate text-sm text-base-content/80">{value}</p>
		{/if}
		{@render content?.()}
	</div>

	<div class="flex-shrink-0">
		{#if actionLabel}
			{#if actionHref}
				<a
					href={actionHref}
					class="inline-flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors
						{danger ? 'text-error hover:bg-error/10' : 'text-brand-400 hover:bg-brand-500/10'}"
				>
					{actionLabel}
					<ChevronRight size={14} />
				</a>
			{:else}
				<button
					type="button"
					class="inline-flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors
						{danger ? 'text-error hover:bg-error/10' : 'text-brand-400 hover:bg-brand-500/10'}
						{loading ? 'cursor-not-allowed opacity-50' : ''}"
					onclick={handleClick}
					disabled={loading}
				>
					{#if loading}
						<span
							class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent"
						></span>
					{/if}
					{actionLabel}
				</button>
			{/if}
		{:else}
			{@render action?.()}
		{/if}
	</div>
</div>
