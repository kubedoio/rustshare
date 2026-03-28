<script lang="ts">
	import { page } from '$app/stores';
	import Logo from '$lib/ui/Logo.svelte';

	interface RailItem {
		icon: string;
		label: string;
		href: string;
	}

	const primaryItems: RailItem[] = [
		{ icon: 'home', label: 'Home', href: '/dashboard' },
		{ icon: 'files', label: 'Files', href: '/files' },
		{ icon: 'activity', label: 'Activity', href: '/notifications' },
	];

	const secondaryItems: RailItem[] = [
		{ icon: 'settings', label: 'Settings', href: '/settings' },
	];

	function isActive(href: string): boolean {
		const pathname = $page.url.pathname;
		if (href === '/dashboard') {
			return pathname === '/dashboard' || pathname === '/';
		}
		return pathname === href || pathname.startsWith(href + '/');
	}
</script>

<!-- Far-left Icon Rail -->
<aside class="hidden w-16 flex-col flex-shrink-0 border-r border-base-300 bg-base-100/80 backdrop-blur lg:flex z-30">
	<!-- Logo -->
	<div class="h-14 flex items-center justify-center border-b border-base-300">
		<a href="/dashboard" class="flex items-center justify-center" aria-label="RustShare">
			<svg class="h-8 w-8" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
				<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor" class="text-brand-500"/>
				<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-400"/>
				<circle cx="24" cy="21" r="5" fill="currentColor" class="text-base-200"/>
				<circle cx="24" cy="21" r="3" fill="currentColor" class="text-brand-500"/>
				<rect x="22.5" y="19.5" width="3" height="3" fill="currentColor" class="text-base-200"/>
			</svg>
		</a>
	</div>

	<!-- Primary Navigation -->
	<nav class="flex-1 py-4 px-2 space-y-1">
		{#each primaryItems as item}
			<a
				href={item.href}
				class="group relative flex items-center justify-center w-12 h-12 rounded-xl transition-all duration-200
					{isActive(item.href) 
						? 'bg-brand-500 text-white shadow-lg shadow-brand-500/25' 
						: 'text-base-content/60 hover:text-base-content hover:bg-base-300'}"
				aria-current={isActive(item.href) ? 'page' : undefined}
				aria-label={item.label}
			>
				<!-- Icon -->
				<span class="w-5 h-5">
					{#if item.icon === 'home'}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
							<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
							<polyline points="9 22 9 12 15 12 15 22"/>
						</svg>
					{:else if item.icon === 'files'}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
							<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
						</svg>
					{:else if item.icon === 'activity'}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
							<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>
						</svg>
					{/if}
				</span>

				<!-- Tooltip -->
				<span class="absolute left-full ml-3 px-2 py-1 bg-base-300 text-xs font-medium text-base-content rounded-lg opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-200 whitespace-nowrap z-50">
					{item.label}
				</span>
			</a>
		{/each}
	</nav>

	<!-- Secondary Navigation -->
	<nav class="py-4 px-2 space-y-1 border-t border-base-300">
		{#each secondaryItems as item}
			<a
				href={item.href}
				class="group relative flex items-center justify-center w-12 h-12 rounded-xl transition-all duration-200
					{isActive(item.href) 
						? 'bg-brand-500 text-white shadow-lg shadow-brand-500/25' 
						: 'text-base-content/60 hover:text-base-content hover:bg-base-300'}"
				aria-current={isActive(item.href) ? 'page' : undefined}
				aria-label={item.label}
			>
				<span class="w-5 h-5">
					{#if item.icon === 'settings'}
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
							<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
							<circle cx="12" cy="12" r="3"/>
						</svg>
					{/if}
				</span>

				<span class="absolute left-full ml-3 px-2 py-1 bg-base-300 text-xs font-medium text-base-content rounded-lg opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all duration-200 whitespace-nowrap z-50">
					{item.label}
				</span>
			</a>
		{/each}
	</nav>
</aside>
