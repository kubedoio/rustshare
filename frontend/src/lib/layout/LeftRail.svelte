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
<aside class="z-30 hidden w-[4.75rem] flex-shrink-0 flex-col border-r border-base-300/80 bg-gradient-to-b from-base-100 via-base-100 to-base-200/55 backdrop-blur lg:flex">
	<!-- Logo -->
	<div class="flex h-16 items-center justify-center border-b border-base-300/80">
		<a href="/dashboard" class="flex items-center justify-center" aria-label="RustShare">
			<svg class="h-9 w-9" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
				<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor" class="text-brand-500"/>
				<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-400"/>
				<circle cx="24" cy="21" r="5" fill="currentColor" class="text-base-200"/>
				<circle cx="24" cy="21" r="3" fill="currentColor" class="text-brand-500"/>
				<rect x="22.5" y="19.5" width="3" height="3" fill="currentColor" class="text-base-200"/>
			</svg>
		</a>
	</div>

	<!-- Primary Navigation -->
	<nav class="flex-1 space-y-2 px-3 py-5">
		{#each primaryItems as item}
			<a
				href={item.href}
					class="group relative flex h-12 w-12 items-center justify-center rounded-2xl border transition-all duration-200
						{isActive(item.href)
							? 'border-brand-500/20 bg-brand-500/12 text-brand-600 shadow-sm shadow-brand-500/15'
							: 'border-transparent text-base-content/55 hover:border-base-300/80 hover:bg-base-100 hover:text-base-content'}"
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
					<span class="invisible absolute left-full z-50 ml-3 whitespace-nowrap rounded-xl border border-base-300 bg-base-100 px-2.5 py-1.5 text-xs font-medium text-base-content opacity-0 shadow-sm transition-all duration-200 group-hover:visible group-hover:opacity-100">
					{item.label}
				</span>
			</a>
		{/each}
	</nav>

	<!-- Secondary Navigation -->
	<nav class="space-y-2 border-t border-base-300/80 px-3 py-5">
		{#each secondaryItems as item}
			<a
				href={item.href}
					class="group relative flex h-12 w-12 items-center justify-center rounded-2xl border transition-all duration-200
						{isActive(item.href)
							? 'border-brand-500/20 bg-brand-500/12 text-brand-600 shadow-sm shadow-brand-500/15'
							: 'border-transparent text-base-content/55 hover:border-base-300/80 hover:bg-base-100 hover:text-base-content'}"
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

					<span class="invisible absolute left-full z-50 ml-3 whitespace-nowrap rounded-xl border border-base-300 bg-base-100 px-2.5 py-1.5 text-xs font-medium text-base-content opacity-0 shadow-sm transition-all duration-200 group-hover:visible group-hover:opacity-100">
					{item.label}
				</span>
			</a>
		{/each}
	</nav>
</aside>
