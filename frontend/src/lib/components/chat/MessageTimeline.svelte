<script lang="ts">
	import type { ChatMessageDto } from '$lib/api/chat';

	interface Props {
		messages: ChatMessageDto[];
		loading: boolean;
		focusTarget: ChatMessageDto | null;
		onLoadMore: () => void;
	}

	let { messages, loading, focusTarget, onLoadMore }: Props = $props();

	let container = $state<HTMLDivElement | null>(null);
	$effect(() => {
		if (!focusTarget || !container) return;
		container
			.querySelector(`[data-message-id="${focusTarget.message_id}"]`)
			?.scrollIntoView({ behavior: 'smooth', block: 'center' });
	});
</script>

<div bind:this={container} class="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
	{#if loading && messages.length === 0}
		<div class="text-sm text-base-content/60">Loading messages…</div>
	{:else if messages.length === 0}
		<div class="text-sm text-base-content/60">No messages yet — say hello.</div>
	{:else}
		<button type="button" class="mb-2 text-sm text-primary" onclick={onLoadMore}>
			Load earlier
		</button>
		{#each messages as message (message.event_id)}
			<div class="mb-3 {message.thread_root_id ? 'ml-6' : ''}" data-message-id={message.message_id}>
				<div class="text-xs text-base-content/50">
					{message.author_pubkey.slice(0, 8)}… · {message.event_created_at}
					{message.thread_root_id ? ' · reply' : ''}
				</div>
				{#if message.body != null}
					<div class="whitespace-pre-wrap text-sm">{message.body}</div>
				{:else}
					<div class="text-sm text-base-content/50 italic">
						Content unavailable in Elembra (reference-only message).
					</div>
				{/if}
				<a
					class="text-xs text-primary"
					href="/ask?scope=chat&communityId={message.community_id}&channelId={message.channel_id}"
					>Ask</a
				>
			</div>
		{/each}
	{/if}
</div>
