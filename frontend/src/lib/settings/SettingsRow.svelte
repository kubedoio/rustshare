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
		onAction = () => {}
	}: Props = $props();

	function handleClick() {
		onAction();
	}
</script>

<div class="flex items-center justify-between py-4 gap-4 {compact ? 'py-3' : ''}">
	<div class="flex-1 min-w-0">
		<p class="text-sm font-medium text-base-content">{label}</p>
		{#if description}
			<p class="text-sm text-base-content/60 mt-0.5">{description}</p>
		{/if}
		{#if value}
			<p class="text-sm text-base-content/80 mt-1 truncate">{value}</p>
		{/if}
		<slot name="content" />
	</div>
	
	<div class="flex-shrink-0">
		{#if actionLabel}
			{#if actionHref}
				<a
					href={actionHref}
					class="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium rounded-lg transition-colors
						{danger 
							? 'text-error hover:bg-error/10' 
							: 'text-brand-400 hover:bg-brand-500/10'}"
				>
					{actionLabel}
					<ChevronRight size={14} />
				</a>
			{:else}
				<button
					type="button"
					class="inline-flex items-center gap-1 px-3 py-1.5 text-sm font-medium rounded-lg transition-colors
						{danger 
							? 'text-error hover:bg-error/10' 
							: 'text-brand-400 hover:bg-brand-500/10'}
						{loading ? 'opacity-50 cursor-not-allowed' : ''}"
					onclick={handleClick}
					disabled={loading}
				>
					{#if loading}
						<span class="inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"></span>
					{/if}
					{actionLabel}
				</button>
			{/if}
		{:else}
			<slot name="action" />
		{/if}
	</div>
</div>
