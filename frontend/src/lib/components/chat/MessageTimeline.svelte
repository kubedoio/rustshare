<script lang="ts">
	import type { ChatAttachmentDto, ChatMessageDto } from '$lib/api/chat';
	import { openChatAttachment } from '$lib/api/chat';

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
		// The focus query can resolve before the channel's page fetch lands;
		// depend on `messages` so this re-runs once the target row renders.
		if (!messages.some((m) => m.message_id === focusTarget.message_id)) return;
		container
			.querySelector(`[data-message-id="${focusTarget.message_id}"]`)
			?.scrollIntoView({ behavior: 'smooth', block: 'center' });
	});

	// Open reauthorizes through the Files owner at read time. Failures are
	// silent: an unauthorized or missing file must not leak existence or
	// ownership to the recipient. The server serves the bytes as a forced
	// download (Content-Disposition: attachment + nosniff); the anchor click
	// below mirrors that client-side — no window.open, so no popup blocker
	// and no reverse-tabnabbing surface.
	async function openAttachment(attachment: ChatAttachmentDto): Promise<void> {
		try {
			const blob = await openChatAttachment(attachment);
			const url = URL.createObjectURL(blob);
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = attachment.resourceId || 'attachment';
			document.body.appendChild(anchor);
			anchor.click();
			URL.revokeObjectURL(url);
			anchor.remove();
		} catch {
			// Existence-hiding by design.
		}
	}
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
				{#if message.attachments.length > 0}
					<div class="mt-1 flex flex-wrap gap-1">
						{#each message.attachments as attachment, index (attachment.application + attachment.resourceType + attachment.resourceId + (attachment.version ?? ''))}
							<button
								type="button"
								class="badge badge-outline gap-1 text-xs"
								title="Open attachment"
								onclick={() => openAttachment(attachment)}
							>
								Attachment{message.attachments.length > 1 ? ` ${index + 1}` : ''}
							</button>
						{/each}
					</div>
				{/if}
				<a
					class="text-xs text-primary"
					href="/ask?scope=chat&communityId={encodeURIComponent(
						message.community_id
					)}&channelId={encodeURIComponent(message.channel_id)}">Ask</a
				>
			</div>
		{/each}
	{/if}
</div>
