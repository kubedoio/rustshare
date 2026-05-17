<script lang="ts">
	let {
		href,
		label,
		active,
		expanded,
		badge = null,
		ariaLabel = undefined
	}: {
		href: string;
		label: string;
		active: boolean;
		expanded: boolean;
		badge?: number | null;
		ariaLabel?: string;
	} = $props();
</script>

<a
	{href}
	class="group relative flex items-center rounded-xl transition-all duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500/50
		{expanded ? 'h-11 w-full gap-3 px-3' : 'h-11 w-11 justify-center'}
		{active
		? 'bg-brand-500/15 text-brand-500 shadow-sm'
		: 'text-base-content/50 hover:bg-base-200 hover:text-base-content'}"
	aria-current={active ? 'page' : undefined}
	aria-label={ariaLabel ?? label}
>
	<slot />

	{#if expanded}
		<span class="flex-1 truncate text-sm font-medium">{label}</span>
		{#if badge !== null && badge > 0}
			<span
				class="inline-flex h-5 min-w-[1.25rem] items-center justify-center rounded-full bg-brand-500 px-1.5 text-[10px] font-bold text-white"
			>
				{badge > 99 ? '99+' : badge}
			</span>
		{/if}
	{:else}
		<!-- Tooltip -->
		<span
			class="invisible absolute left-full z-50 ml-3 rounded-lg border border-base-300/70 bg-base-100 px-2.5 py-1.5 text-xs font-medium whitespace-nowrap text-base-content opacity-0 shadow-lg transition-all duration-200 group-hover:visible group-hover:opacity-100"
		>
			{label}
		</span>
	{/if}
</a>
