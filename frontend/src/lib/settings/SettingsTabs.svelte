<script lang="ts" module>
	export type TabId =
		'general' | 'security' | 'devices' | 'appearance' | 'sharing' | 'activity' | 'modules' | 'mail';
</script>

<script lang="ts">
	import {
		User,
		Shield,
		Smartphone,
		Palette,
		Share2,
		Activity,
		LayoutGrid,
		Mail
	} from 'lucide-svelte';

	let {
		activeTab = 'general',
		onTabChange
	}: {
		activeTab?: TabId;
		onTabChange: (tab: TabId) => void;
	} = $props();

	interface Tab {
		id: TabId;
		label: string;
		icon: typeof User;
	}

	const tabs: Tab[] = [
		{ id: 'general', label: 'General', icon: User },
		{ id: 'security', label: 'Security', icon: Shield },
		{ id: 'devices', label: 'Devices', icon: Smartphone },
		{ id: 'appearance', label: 'Appearance', icon: Palette },
		{ id: 'sharing', label: 'Sharing', icon: Share2 },
		{ id: 'activity', label: 'Activity', icon: Activity },
		{ id: 'modules', label: 'Modules', icon: LayoutGrid },
		{ id: 'mail', label: 'Mail', icon: Mail }
	];

	function handleTabClick(tabId: TabId) {
		onTabChange(tabId);
	}
</script>

<!-- Desktop Tabs -->
<div class="hidden border-b border-base-300 sm:block">
	<nav class="flex gap-1" aria-label="Settings tabs">
		{#each tabs as tab}
			<button
				type="button"
				class="flex items-center gap-2 border-b-2 px-4 py-3 text-sm font-medium whitespace-nowrap transition-colors
					{activeTab === tab.id
					? 'border-brand-500 text-brand-400'
					: 'border-transparent text-base-content/60 hover:border-base-300 hover:text-base-content'}"
				onclick={() => handleTabClick(tab.id)}
				aria-current={activeTab === tab.id ? 'page' : undefined}
			>
				<svelte:component this={tab.icon} size={16} />
				{tab.label}
			</button>
		{/each}
	</nav>
</div>

<!-- Mobile Tabs - Horizontal Scroll -->
<div class="overflow-x-auto border-b border-base-300 sm:hidden">
	<nav class="flex min-w-max gap-1 px-2" aria-label="Settings tabs">
		{#each tabs as tab}
			<button
				type="button"
				class="flex items-center gap-2 border-b-2 px-3 py-3 text-sm font-medium whitespace-nowrap transition-colors
					{activeTab === tab.id
					? 'border-brand-500 text-brand-400'
					: 'border-transparent text-base-content/60 hover:border-base-300 hover:text-base-content'}"
				onclick={() => handleTabClick(tab.id)}
				aria-current={activeTab === tab.id ? 'page' : undefined}
			>
				<svelte:component this={tab.icon} size={16} />
				{tab.label}
			</button>
		{/each}
	</nav>
</div>
