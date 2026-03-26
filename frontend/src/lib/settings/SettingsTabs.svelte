<script lang="ts">
	import { User, Shield, Bell, Smartphone, Palette, Share2, LogOut } from 'lucide-svelte';

	export type TabId = 'general' | 'security' | 'notifications' | 'devices' | 'appearance' | 'sharing';

	export let activeTab: TabId = 'general';
	export let onTabChange: (tab: TabId) => void;

	interface Tab {
		id: TabId;
		label: string;
		icon: typeof User;
	}

	const tabs: Tab[] = [
		{ id: 'general', label: 'General', icon: User },
		{ id: 'security', label: 'Security', icon: Shield },
		{ id: 'notifications', label: 'Notifications', icon: Bell },
		{ id: 'devices', label: 'Devices', icon: Smartphone },
		{ id: 'appearance', label: 'Appearance', icon: Palette },
		{ id: 'sharing', label: 'Sharing', icon: Share2 },
	];

	function handleTabClick(tabId: TabId) {
		onTabChange(tabId);
	}
</script>

<!-- Desktop Tabs -->
<div class="hidden sm:block border-b border-base-300">
	<nav class="flex gap-1" aria-label="Settings tabs">
		{#each tabs as tab}
			<button
				type="button"
				class="flex items-center gap-2 px-4 py-3 text-sm font-medium border-b-2 transition-colors
					{activeTab === tab.id 
						? 'border-brand-500 text-brand-400' 
						: 'border-transparent text-base-content/60 hover:text-base-content hover:border-base-300'}"
				on:click={() => handleTabClick(tab.id)}
				aria-current={activeTab === tab.id ? 'page' : undefined}
			>
				<svelte:component this={tab.icon} size={16} />
				{tab.label}
			</button>
		{/each}
	</nav>
</div>

<!-- Mobile Tabs - Horizontal Scroll -->
<div class="sm:hidden border-b border-base-300 overflow-x-auto">
	<nav class="flex gap-1 min-w-max px-2" aria-label="Settings tabs">
		{#each tabs as tab}
			<button
				type="button"
				class="flex items-center gap-2 px-3 py-3 text-sm font-medium border-b-2 transition-colors whitespace-nowrap
					{activeTab === tab.id 
						? 'border-brand-500 text-brand-400' 
						: 'border-transparent text-base-content/60 hover:text-base-content hover:border-base-300'}"
				on:click={() => handleTabClick(tab.id)}
				aria-current={activeTab === tab.id ? 'page' : undefined}
			>
				<svelte:component this={tab.icon} size={16} />
				{tab.label}
			</button>
		{/each}
	</nav>
</div>
