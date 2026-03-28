<script lang="ts">
	import { page } from '$app/stores';
	
	export let href: string;
	export let icon: string;
	export let label: string;
	export let badge: number | null = null;
	export let compact = false;
	export let onClick: (() => void) | undefined = undefined;

	$: isActive = $page.url.pathname === href || $page.url.pathname.startsWith(href + '/');
</script>

<a
	{href}
	class="group flex items-center gap-3 rounded-xl border px-3 py-2.5 transition-all duration-200
		{isActive
			? 'border-brand-500/20 bg-brand-500/10 text-brand-600'
			: 'border-transparent text-base-content/70 hover:border-base-300/70 hover:bg-base-200/55 hover:text-base-content'}
		{compact ? 'justify-center' : ''}"
	on:click={onClick}
	aria-current={isActive ? 'page' : undefined}
>
	<span class="flex-shrink-0 {isActive ? 'text-brand-600' : 'text-base-content/45 group-hover:text-base-content/70'}">
		{#if icon === 'home'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
				<polyline points="9 22 9 12 15 12 15 22"/>
			</svg>
		{:else if icon === 'files'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>
			</svg>
		{:else if icon === 'activity'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>
			</svg>
		{:else if icon === 'share'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/>
				<polyline points="16 6 12 2 8 6"/>
				<line x1="12" x2="12" y1="2" y2="15"/>
			</svg>
		{:else if icon === 'users'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
				<circle cx="9" cy="7" r="4"/>
				<path d="M22 21v-2a4 4 0 0 0-3-3.87"/>
				<path d="M16 3.13a4 4 0 0 1 0 7.75"/>
			</svg>
		{:else if icon === 'bell'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/>
				<path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>
			</svg>
		{:else if icon === 'settings'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
				<circle cx="12" cy="12" r="3"/>
			</svg>
		{:else if icon === 'trash'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M3 6h18"/>
				<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
				<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
				<line x1="10" x2="10" y1="11" y2="17"/>
				<line x1="14" x2="14" y1="11" y2="17"/>
			</svg>
		{:else if icon === 'image'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>
				<circle cx="9" cy="9" r="2"/>
				<path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>
			</svg>
		{:else if icon === 'clock'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<circle cx="12" cy="12" r="10"/>
				<polyline points="12 6 12 12 16 14"/>
			</svg>
		{:else if icon === 'star'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
			</svg>
		{:else if icon === 'plus'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<path d="M5 12h14"/>
				<path d="M12 5v14"/>
			</svg>
		{:else if icon === 'search'}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<circle cx="11" cy="11" r="8"/>
				<path d="m21 21-4.3-4.3"/>
			</svg>
		{:else}
			<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
				<circle cx="12" cy="12" r="10"/>
			</svg>
		{/if}
	</span>
	
	{#if !compact}
		<span class="flex-1 truncate font-data text-sm font-medium tracking-[0.01em]">{label}</span>
		
		{#if badge !== null && badge > 0}
			<span class="ml-auto inline-flex h-5 min-w-[1.25rem] flex-shrink-0 items-center justify-center rounded-full bg-brand-500 px-1.5 text-xs font-semibold text-white">
				{badge > 99 ? '99+' : badge}
			</span>
		{/if}
	{/if}
</a>
