<script lang="ts">
	import type { ChatChannelInfo } from '$lib/api/chat';
	import { Hash, Lock } from 'lucide-svelte';

	interface Props {
		channels: ChatChannelInfo[];
		loading: boolean;
		selectedChannelId: string | null;
		onSelect: (channelId: string) => void;
	}

	let { channels, loading, selectedChannelId, onSelect }: Props = $props();

	function displayName(channel: ChatChannelInfo): string {
		return channel.name ?? channel.channel_id;
	}

	function isSelected(channel: ChatChannelInfo): boolean {
		return channel.channel_id === selectedChannelId;
	}
</script>

<aside class="flex h-full w-60 shrink-0 flex-col border-r border-base-300 bg-base-100">
	<div class="px-3 py-3">
		<h2 class="text-xs font-bold uppercase tracking-wider text-base-content/50">Channels</h2>
	</div>
	<div class="flex-1 overflow-y-auto px-2 pb-2">
		{#if loading}
			<ul class="space-y-1" aria-busy="true" aria-label="Loading channels">
				{#each Array.from({ length: 6 }) as _, i (i)}
					<li class="h-8 animate-pulse rounded bg-base-200"></li>
				{/each}
			</ul>
		{:else if channels.length === 0}
			<p class="px-2 py-1 text-sm text-base-content/60">No channels yet.</p>
		{:else}
			<ul class="space-y-0.5" role="listbox" aria-label="Channels">
				{#each channels as channel (channel.channel_id)}
					<li role="presentation">
						<button
							type="button"
							role="option"
							aria-selected={isSelected(channel)}
							class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors
								{isSelected(channel)
								? 'bg-base-200 font-medium text-primary'
								: 'text-base-content hover:bg-base-200/60'}"
							title={displayName(channel)}
							onclick={() => onSelect(channel.channel_id)}
						>
							<Hash size={14} class="shrink-0 text-base-content/40" />
							<span class="min-w-0 flex-1 truncate">{displayName(channel)}</span>
							{#if channel.visibility === 'private'}
								<Lock
									size={12}
									class="shrink-0 text-base-content/40"
									aria-label="Private channel"
								/>
							{/if}
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</aside>
