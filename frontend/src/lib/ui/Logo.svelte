<script lang="ts">
	export let size: 'sm' | 'md' | 'lg' | 'xl' = 'md';
	export let showText = true;
	export let className = '';

	const sizeClasses = {
		sm: 'h-6',
		md: 'h-8',
		lg: 'h-10',
		xl: 'h-12'
	};

	const textSizes = {
		sm: 'text-lg',
		md: 'text-xl',
		lg: 'text-2xl',
		xl: 'text-3xl'
	};
</script>

<a href="/" class="flex items-center gap-2.5 {className}" aria-label="RustShare Home">
	<svg
		class="{sizeClasses[size]} w-auto flex-shrink-0"
		viewBox="0 0 32 32"
		fill="none"
		xmlns="http://www.w3.org/2000/svg"
	>
		<!-- Folder base -->
		<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor" class="text-brand-500" />
		<!-- Folder tab -->
		<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-400" />
		<!-- Gear outer -->
		<circle cx="24" cy="21" r="5" fill="currentColor" class="text-base-100" />
		<!-- Gear inner -->
		<circle cx="24" cy="21" r="3" fill="currentColor" class="text-brand-500" />
		<!-- Gear center -->
		<rect x="22.5" y="19.5" width="3" height="3" fill="currentColor" class="text-base-100" />
	</svg>

	{#if showText}
		<span class="font-bold tracking-tight text-base-content {textSizes[size]}"> RustShare </span>
	{/if}
</a>
