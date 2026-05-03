<script lang="ts">
	import { ChevronRight } from 'lucide-svelte';

	interface Props {
		title: string;
		subtitle?: string;
		breadcrumb?: Array<{ label: string; onClick?: () => void }>;
		metadata?: string;
	}

	let { title, subtitle, breadcrumb, metadata }: Props = $props();
</script>

<div class="flex flex-col gap-6">
	<!-- Header -->
	<div
		class="flex flex-col gap-4 rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-4 sm:flex-row sm:items-center sm:justify-between lg:p-5"
	>
		<div class="flex min-w-0 flex-col gap-1">
			{#if breadcrumb && breadcrumb.length > 0}
				<nav aria-label="Breadcrumb" class="flex flex-wrap items-center gap-0.5">
					{#each breadcrumb as crumb, index}
						{@const isLast = index === breadcrumb.length - 1}
						{#if crumb.onClick && !isLast}
							<button
								type="button"
								class="rounded-md px-1.5 py-0.5 text-sm font-medium text-base-content/70 transition-colors hover:bg-brand-500/10 hover:text-brand-600"
								onclick={crumb.onClick}
							>
								{crumb.label}
							</button>
						{:else}
							<span
								class="rounded-md px-1.5 py-0.5 text-sm {isLast
									? 'font-semibold text-base-content'
									: 'text-base-content/60'}"
								aria-current={isLast ? 'page' : undefined}
							>
								{crumb.label}
							</span>
						{/if}
						{#if !isLast}
							<ChevronRight size={14} class="flex-shrink-0 text-base-content/30" />
						{/if}
					{/each}
				</nav>
			{/if}

			<h1 class="text-xl font-semibold text-base-content">{title}</h1>

			{#if subtitle}
				<p class="text-sm text-base-content/60">{subtitle}</p>
			{/if}

			{#if metadata}
				<span class="text-xs text-base-content/40">{metadata}</span>
			{/if}
		</div>

		<div class="flex flex-wrap items-center gap-2">
			<slot name="primaryAction" />
			<slot name="secondaryActions" />
			<slot name="overflowActions" />
		</div>
	</div>

	<!-- Content -->
	<div class="flex flex-col gap-6">
		<slot />
	</div>
</div>
