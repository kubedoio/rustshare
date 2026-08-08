<script lang="ts" module>
	export type TabId =
		| 'general'
		| 'security'
		| 'devices'
		| 'appearance'
		| 'sharing'
		| 'activity'
		| 'applications'
		| 'mail';
</script>

<script lang="ts">
	import { User, Shield, Smartphone, Palette, Share2, Activity, LayoutGrid } from 'lucide-svelte';

	interface ApplicationSettingLink {
		id: string;
		label: string;
		route: string;
	}

	let {
		activeTab = 'general',
		onTabChange,
		applicationSettings = []
	}: {
		activeTab?: TabId;
		onTabChange: (tab: TabId) => void;
		applicationSettings?: ApplicationSettingLink[];
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
		{ id: 'applications', label: 'Applications', icon: LayoutGrid }
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

{#if applicationSettings?.length}
	<div class="border-b border-base-300 px-2 py-2 sm:px-0">
		<nav class="flex flex-wrap gap-1" aria-label="Application settings">
			{#each applicationSettings as setting}
				<a
					href={setting.route}
					class="flex items-center gap-2 rounded-md px-3 py-2 text-sm text-base-content/60 hover:bg-base-200 hover:text-base-content"
				>
					{setting.label}
				</a>
			{/each}
		</nav>
	</div>
{/if}

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

{#if applicationSettings?.length}
	<div class="border-b border-base-300 px-2 py-2 sm:hidden">
		<nav class="flex min-w-max gap-1" aria-label="Application settings">
			{#each applicationSettings as setting}
				<a
					href={setting.route}
					class="rounded-md px-3 py-2 text-sm text-base-content/60 hover:bg-base-200 hover:text-base-content"
				>
					{setting.label}
				</a>
			{/each}
		</nav>
	</div>
{/if}
