<script lang="ts">
	import type { ChatChannelInfo } from '$lib/api/chat';

	interface Props {
		channels: ChatChannelInfo[];
		loading: boolean;
		selectedChannelId: string | null;
		onSelect: (channelId: string) => void;
	}

	let { channels, loading, selectedChannelId, onSelect }: Props = $props();
</script>

<aside class="w-56 shrink-0 border-r border-base-300 p-2">
	{#if loading}
		<div class="text-sm text-base-content/60">Loading channels…</div>
	{:else if channels.length === 0}
		<div class="text-sm text-base-content/60">No channels yet.</div>
	{:else}
		<ul>
			{#each channels as channel (channel.channel_id)}
				<li>
					<button
						type="button"
						class="w-full rounded px-2 py-1 text-left text-sm {channel.channel_id ===
						selectedChannelId
							? 'bg-base-200 font-medium'
							: 'hover:bg-base-200/60'}"
						onclick={() => onSelect(channel.channel_id)}
					>
						# {channel.channel_id}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</aside>
