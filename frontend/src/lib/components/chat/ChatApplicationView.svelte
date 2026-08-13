<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import {
		getChatStatus,
		getChatChannels,
		getChatMessages,
		getChatMessage,
		type ChatMessageDto,
		type ChatMessagesPage
	} from '$lib/api/chat';
	import BindingPanel from './BindingPanel.svelte';
	import ChannelList from './ChannelList.svelte';
	import MessageTimeline from './MessageTimeline.svelte';
	import MessageComposer from './MessageComposer.svelte';

	const statusQuery = createQuery({
		queryKey: ['chat-status'],
		queryFn: () => getChatStatus(),
		staleTime: 30_000
	});
	const channelsQuery = createQuery({
		queryKey: ['chat-channels'],
		queryFn: () => getChatChannels(),
		enabled: false
	});

	let selectedChannelId = $state<string | null>(null);
	let focusedMessageId = $state<string | null>(null);
	let cursor = $state<string | null>(null);

	// Deep links: /apps/chat?channel=<id>&message=<id>
	$effect(() => {
		const params = $page.url.searchParams;
		const channel = params.get('channel');
		const message = params.get('message');
		if (channel) selectedChannelId = channel;
		if (message) focusedMessageId = message;
	});

	$effect(() => {
		channelsQuery.setOptions({
			queryKey: ['chat-channels'],
			queryFn: () => getChatChannels(),
			enabled: $statusQuery.data?.mapping != null && $statusQuery.data?.binding != null
		});
	});

	$effect(() => {
		const channels = $channelsQuery.data;
		if (!channels || channels.length === 0) return;
		if (selectedChannelId && channels.some((c) => c.channel_id === selectedChannelId)) return;
		selectedChannelId = channels[0].channel_id;
	});

	// A citation deep link names only the message; once it resolves, switch to
	// the channel that owns it so focus/scroll lands in the right timeline.
	$effect(() => {
		const message = $focusedMessageQuery.data;
		if (message && message.channel_id !== selectedChannelId) {
			selectedChannelId = message.channel_id;
		}
	});

	const messagesQuery = createQuery<ChatMessagesPage>({
		queryKey: ['chat-messages', null, null],
		queryFn: () => Promise.resolve({ messages: [], next_before: null }),
		enabled: false
	});

	const focusedMessageQuery = createQuery<ChatMessageDto | null>({
		queryKey: ['chat-message', null],
		queryFn: () => Promise.resolve(null),
		enabled: false
	});

	$effect(() => {
		messagesQuery.setOptions({
			queryKey: ['chat-messages', selectedChannelId, cursor],
			queryFn: () => getChatMessages(selectedChannelId!, cursor),
			enabled: selectedChannelId != null
		});
	});

	$effect(() => {
		focusedMessageQuery.setOptions({
			queryKey: ['chat-message', focusedMessageId],
			queryFn: () => getChatMessage(focusedMessageId!),
			enabled: focusedMessageId != null
		});
	});

	// Polling fallback: 15 s while mounted, regardless of websocket state.
	onMount(() => {
		const interval = setInterval(() => {
			if (selectedChannelId) messagesQuery.refetch();
		}, 15_000);
		return () => clearInterval(interval);
	});

	function handleSendFailure(message: string): void {
		relayError = message;
	}

	let relayError = $state('');

	const status = $derived($statusQuery.data);
	const bindingActive = $derived(status?.binding != null && status.binding.status === 'Active');
	const askChannelHref = $derived(
		selectedChannelId && status?.mapping
			? `/ask?scope=chat&communityId=${encodeURIComponent(status.mapping.community_id)}&channelId=${encodeURIComponent(selectedChannelId)}`
			: null
	);
	const focusTarget: ChatMessageDto | null = $derived(
		$focusedMessageQuery.data && $focusedMessageQuery.data.channel_id === selectedChannelId
			? $focusedMessageQuery.data
			: null
	);
</script>

{#if $statusQuery.isLoading}
	<div class="p-6 text-base-content/60">Loading Chat…</div>
{:else if !status || !status.chat_enabled}
	<div class="p-6 text-base-content/60">Chat is not enabled for this workspace.</div>
{:else if !status.mapping}
	<div class="p-6 text-base-content/60">
		No Buzz community is mapped for this workspace yet. An administrator can configure it.
	</div>
{:else if !bindingActive}
	<BindingPanel
		onBound={() => {
			statusQuery.refetch();
			channelsQuery.refetch();
		}}
	/>
{:else if !status.admission_active}
	<div class="p-6 text-base-content/60">
		Your Chat admission is still being processed by the community relay.
	</div>
{:else}
	<div class="flex h-full">
		<ChannelList
			channels={$channelsQuery.data ?? []}
			loading={$channelsQuery.isLoading}
			{selectedChannelId}
			onSelect={(id: string) => {
				selectedChannelId = id;
				focusedMessageId = null;
				cursor = null;
			}}
		/>
		<div class="flex min-w-0 flex-1 flex-col">
			{#if askChannelHref}
				<div class="px-4 pt-2">
					<a class="text-sm text-primary" href={askChannelHref}>Ask this channel</a>
				</div>
			{/if}
			<MessageTimeline
				messages={$messagesQuery.data?.messages ?? []}
				loading={$messagesQuery.isLoading}
				{focusTarget}
				onLoadMore={() => {
					cursor = $messagesQuery.data?.next_before ?? null;
				}}
			/>
			<MessageComposer
				relayUrl={status.mapping.relay_url}
				channelId={selectedChannelId ?? ''}
				onSendFailure={handleSendFailure}
			/>
			{#if relayError}
				<div class="px-4 py-2 text-sm text-error">
					Relay unreachable — message not sent. Reads are unaffected.
				</div>
			{/if}
		</div>
	</div>
{/if}
