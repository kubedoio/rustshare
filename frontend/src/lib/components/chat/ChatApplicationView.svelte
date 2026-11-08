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
	import ChannelHeader from './ChannelHeader.svelte';
	import ChannelPermissions from './ChannelPermissions.svelte';
	import CreateChannelPanel from './CreateChannelPanel.svelte';
	import MessageTimeline from './MessageTimeline.svelte';
	import MessageComposer from './MessageComposer.svelte';
	import ChatIdentityUnlock from './ChatIdentityUnlock.svelte';
	import { chatSessionStore, lock, getSigningKey } from '$lib/chat/session';
	import { getChatCapabilities } from '$lib/chat/capabilities';
	import { listVisibleChannels, type ChannelDirectoryResult } from '$lib/chat/channels';

	$effect(() => {
		const session = $chatSessionStore;
		if (
			$statusQuery.data &&
			session.state === 'unlocked' &&
			session.pubkey !== $statusQuery.data.binding?.buzz_pubkey
		)
			lock();
	});

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
	// Relay-backed directory: group discovery via kinds-[39000] historical REQ
	// lists every channel this identity may see, including channels with zero
	// messages. The relay applies access control before returning, so private
	// channels the caller may not see never reach this query's result.
	const directoryQuery = createQuery<ChannelDirectoryResult>({
		queryKey: ['chat-channel-directory', null, null],
		queryFn: () => Promise.resolve({ ok: true, channels: [] }),
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

	// Keep the URL addressable without adding history entries: reloading the
	// page must reconstruct selection from the relay directory, not from any
	// local cache, so the URL is a view parameter only.
	$effect(() => {
		const id = selectedChannelId;
		if (!id) return;
		try {
			const url = new URL($page.url);
			if (url.searchParams.get('channel') === id) return;
			url.searchParams.set('channel', id);
			history.replaceState(history.state, '', url);
		} catch {
			/* History unavailable (e.g. embedded); selection still works. */
		}
	});

	$effect(() => {
		channelsQuery.setOptions({
			queryKey: ['chat-channels'],
			queryFn: () => getChatChannels(),
			enabled: $statusQuery.data?.mapping != null && $statusQuery.data?.binding != null
		});
	});

	const status = $derived($statusQuery.data);
	const capabilities = $derived(getChatCapabilities(status?.runtime_profile ?? 'upstream_managed'));
	const directoryEnabled = $derived(
		capabilities.directory.available &&
			status?.mapping?.relay_url != null &&
			status.mapping.relay_pubkey != null &&
			$chatSessionStore.state === 'unlocked'
	);

	$effect(() => {
		directoryQuery.setOptions({
			queryKey: [
				'chat-channel-directory',
				status?.mapping?.relay_url ?? null,
				status?.mapping?.relay_pubkey ?? null
			],
			queryFn: () => {
				const mapping = status?.mapping;
				const key = getSigningKey();
				if (!mapping?.relay_url || !mapping.relay_pubkey || !key)
					return Promise.resolve({ ok: true as const, channels: [] });
				return listVisibleChannels({
					relayUrl: mapping.relay_url,
					secretKey: key,
					relayPubkey: mapping.relay_pubkey
				});
			},
			enabled: directoryEnabled,
			staleTime: 15_000,
			placeholderData: (previous) => previous
		});
	});

	// Relay directory entries merged with the observation-derived channel list.
	// Observation rows win for shared ids (they carry latest_event_at); the
	// relay side contributes channels that have no observed messages yet.
	const relayChannels = $derived.by(() => {
		const data = $directoryQuery.data;
		if (!data?.ok) return [];
		return data.channels.map((entry) => ({
			channel_id: entry.channelId,
			name: entry.name,
			channel_kind: 'stream',
			channel_type: entry.channelType,
			visibility: entry.visibility,
			member: null,
			latest_event_at: ''
		}));
	});
	const channelEntries = $derived.by(() => {
		const observed = $channelsQuery.data ?? [];
		const observedIds = new Set(observed.map((channel) => channel.channel_id));
		return [
			...observed,
			...relayChannels.filter((channel) => !observedIds.has(channel.channel_id))
		];
	});
	// Selection may only be validated once the directory outcome is known;
	// otherwise a deep link to an empty (relay-only) channel would be clobbered
	// while the discovery query is still in flight.
	const directoryReady = $derived(!directoryEnabled || $directoryQuery.isFetched);

	$effect(() => {
		const entries = channelEntries;
		if (!directoryReady || entries.length === 0) return;
		if (selectedChannelId && entries.some((c) => c.channel_id === selectedChannelId)) return;
		selectedChannelId = entries[0].channel_id;
	});

	// A citation deep link names only the message; once it resolves, switch to
	// the channel that owns it so focus/scroll lands in the right timeline.
	$effect(() => {
		const message = $focusedMessageQuery.data;
		if (message && message.channel_id !== selectedChannelId) {
			selectedChannelId = message.channel_id;
			// A channel-scoped cursor from the previous channel must not leak
			// into the focus target's channel.
			cursor = null;
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
			enabled: selectedChannelId != null,
			// Keep the current page visible while an older page loads, so
			// pagination never flashes an empty timeline or hides recent
			// messages.
			placeholderData: (previous, previousQuery) =>
				previousQuery?.queryKey[1] === selectedChannelId ? previous : undefined
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
	// Messages keep the open timeline current; channels need the same fallback
	// or a dead websocket freezes the channel list forever (channels are only
	// invalidated over WS). The guard mirrors channelsQuery's enabled
	// condition, so the poll never fetches while Chat is unbound.
	onMount(() => {
		const interval = setInterval(() => {
			statusQuery.refetch();
			if (selectedChannelId) messagesQuery.refetch();
			const status = $statusQuery.data;
			if (status?.mapping != null && status?.binding != null) channelsQuery.refetch();
			if (directoryEnabled) directoryQuery.refetch();
		}, 15_000);
		return () => clearInterval(interval);
	});

	let pendingEventId = $state<string | null>(null);
	let syncState = $state<'idle' | 'waiting' | 'observed' | 'warning'>('idle');
	let accumulatedMessages = $state<ChatMessageDto[]>([]);
	// Accumulate older message pages when paginating backward.
	// `prevCursor` uses a module-closure variable (not $state) to avoid
	// effect_update_depth_exceeded from reading and writing the same reactive
	// dependency within one $effect; `lastNextBefore` is write-only in the
	// effect, so it can be reactive for the template.
	let prevCursor: string | null = null;
	let lastNextBefore = $state<string | null>(null);
	$effect(() => {
		const page = $messagesQuery.data;
		if (!page) return;
		if ($messagesQuery.isPlaceholderData) {
			// keepPreviousData shows the previous page while an older page is
			// loading; accumulating it would prepend the current page onto
			// itself and swallow the real older page when it arrives.
			return;
		}
		if (cursor === null) {
			// Latest page or channel switch: replace
			accumulatedMessages = page.messages;
		} else if (cursor !== prevCursor) {
			// Older page loaded: prepend without duplicates
			const existingIds = new Set(accumulatedMessages.map((m) => m.event_id));
			accumulatedMessages = [
				...page.messages.filter((m) => !existingIds.has(m.event_id)),
				...accumulatedMessages
			];
		} else {
			// cursor === prevCursor: same page loaded again (polling), keep
			// accumulation but refresh the cursor for the next older page.
			lastNextBefore = page.next_before;
			return;
		}
		prevCursor = cursor;
		lastNextBefore = page.next_before;
	});

	// The success banner is informational: auto-clear it shortly after the
	// message is observed, so the status row does not linger forever. Re-sends
	// and channel switches invalidate the timer through the effect teardown.
	$effect(() => {
		if (syncState !== 'observed') return;
		const timer = setTimeout(() => {
			syncState = 'idle';
		}, 3_000);
		return () => clearTimeout(timer);
	});

	// A channel switch orphans any in-flight send: its event belongs to the
	// previous channel's timeline. Reset the pending-event tracking (and the
	// 15 s warning timer that watches it) so the status cannot hang on stale
	// state; the warning persists only until the user sends again or switches.
	$effect(() => {
		if (selectedChannelId !== null) {
			accumulatedMessages = [];
			pendingEventId = null;
			syncState = 'idle';
			relayError = '';
		}
	});

	$effect(() => {
		if (!pendingEventId) return;
		if ($messagesQuery.data?.messages.some((m) => m.event_id === pendingEventId)) {
			pendingEventId = null;
			syncState = 'observed';
		}
	});
	$effect(() => {
		if (!pendingEventId) return;
		const timer = setTimeout(() => {
			if (pendingEventId) syncState = 'warning';
		}, 15_000);
		return () => clearTimeout(timer);
	});

	function handleSendFailure(message: string): void {
		relayError = message;
	}

	let relayError = $state('');
	let postingAllowed = $state(false);
	$effect(() => {
		void selectedChannelId;
		void $chatSessionStore;
		postingAllowed = false;
	});

	const bindingActive = $derived(status?.binding != null && status.binding.status === 'Active');
	const selectedChannel = $derived(
		channelEntries.find((c) => c.channel_id === selectedChannelId) ?? null
	);
	const channelDisplayName = $derived(selectedChannel?.name ?? selectedChannelId ?? 'unknown');
	const askChannelHref = $derived(
		selectedChannelId && status?.mapping && status.ask_available
			? `/ask?scope=chat&communityId=${encodeURIComponent(status.mapping.community_id)}&channelId=${encodeURIComponent(selectedChannelId)}`
			: null
	);
	const focusTarget: ChatMessageDto | null = $derived(
		$focusedMessageQuery.data && $focusedMessageQuery.data.channel_id === selectedChannelId
			? $focusedMessageQuery.data
			: null
	);
	const hasMoreMessages = $derived(lastNextBefore != null);
</script>

{#if $statusQuery.isLoading}
	<div class="p-6 text-base-content/60">Loading Chat…</div>
{:else if !status || !status.chat_enabled}
	<div class="p-6 text-base-content/60">Chat is not enabled for this workspace.</div>
{:else if !status.mapping}
	<div class="p-6 text-base-content/60">Chat is being configured for this workspace.</div>
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
	<div class="flex h-full min-h-0">
		<ChannelList
			channels={channelEntries}
			loading={$channelsQuery.isLoading}
			{selectedChannelId}
			onSelect={(id: string) => {
				selectedChannelId = id;
				focusedMessageId = null;
				cursor = null;
			}}
		>
			{#snippet header()}
				{#if capabilities.createChannel.available && status.mapping?.relay_url && status.mapping.relay_pubkey}
					<CreateChannelPanel
						relayUrl={status.mapping.relay_url}
						relayPubkey={status.mapping.relay_pubkey}
						onCreated={(id: string) => {
							selectedChannelId = id;
							directoryQuery.refetch();
						}}
					/>
				{/if}
			{/snippet}
		</ChannelList>
		<div class="flex min-w-0 flex-1 flex-col">
			<ChannelHeader
				channelId={selectedChannelId ?? ''}
				channelName={channelDisplayName}
				channelKind={selectedChannel?.channel_kind ?? null}
				visibility={selectedChannel?.visibility ?? null}
				member={selectedChannel?.member ?? null}
				askAvailable={status.ask_available}
				askHref={askChannelHref ?? undefined}
				communityId={status.mapping?.community_id ?? ''}
			/>

			{#if status.runtime_profile === 'fork_client_held' && status.mapping.relay_pubkey && selectedChannelId && $chatSessionStore.state === 'unlocked'}
				{#key `${selectedChannelId}:${status.mapping.relay_url}:${status.mapping.relay_pubkey}:${$chatSessionStore.pubkey}`}
					<ChannelPermissions
						relayUrl={status.mapping.relay_url}
						relayPubkey={status.mapping.relay_pubkey}
						channelId={selectedChannelId}
						pubkey={$chatSessionStore.pubkey}
						onPostingChange={(allowed) => (postingAllowed = allowed)}
						onChanged={() => directoryQuery.refetch()}
					/>
				{/key}
			{/if}

			{#if syncState !== 'idle' || cursor || relayError}
				<div class="border-b border-base-300 bg-base-100 px-4 py-2">
					<div class="flex flex-wrap items-center gap-3 text-sm">
						{#if syncState === 'waiting'}
							<span class="text-base-content/60" role="status">
								Sent — waiting for Elembra sync…
							</span>
						{:else if syncState === 'warning'}
							<span class="text-warning" role="status">
								Sent, but Elembra has not observed it yet. Check Chat diagnostics.
							</span>
						{:else if syncState === 'observed'}
							<span class="text-success" role="status">Observed by Elembra.</span>
						{/if}
						{#if cursor}
							<button
								type="button"
								class="text-sm text-primary hover:underline"
								onclick={() => (cursor = null)}
							>
								Back to latest
							</button>
						{/if}
					</div>
					{#if relayError}
						<p class="mt-1 text-sm text-error">{relayError}</p>
					{/if}
				</div>
			{/if}

			<MessageTimeline
				messages={accumulatedMessages}
				loading={$messagesQuery.isLoading}
				{focusTarget}
				hasMore={hasMoreMessages}
				askAvailable={status.ask_available}
				communityId={status.mapping?.community_id ?? ''}
				onLoadMore={() => {
					if ($messagesQuery.isFetching) return;
					const next = lastNextBefore;
					if (next == null) return;
					if ($messagesQuery.isError) {
						// The fetch for the current cursor failed; re-assigning
						// the same cursor would be a no-op, so refetch explicitly.
						messagesQuery.refetch();
					} else {
						cursor = next;
					}
				}}
			/>
			{#if $chatSessionStore.state === 'unlocked' && $chatSessionStore.pubkey === status.binding?.buzz_pubkey}
				{#key `${selectedChannelId}:${status.mapping.relay_url}:${status.binding?.buzz_pubkey}`}
					<MessageComposer
						relayUrl={status.mapping.relay_url}
						channelId={selectedChannelId ?? ''}
						channelName={channelDisplayName}
						boundPubkey={status.binding?.buzz_pubkey ?? ''}
						postingAllowed={status.runtime_profile === 'fork_client_held'
							? postingAllowed &&
								selectedChannel != null &&
								!$statusQuery.isError &&
								!$channelsQuery.isError
							: status.runtime_profile === 'local_client_held' &&
								selectedChannel != null &&
								selectedChannel.member !== false}
						observedEventIds={accumulatedMessages.map((message) => message.event_id)}
						onSendFailure={handleSendFailure}
						onSent={(eventId: string) => {
							cursor = null;
							pendingEventId = eventId;
							syncState = 'waiting';
						}}
					/>
				{/key}
			{:else}
				<ChatIdentityUnlock
					boundPubkey={status.binding?.buzz_pubkey ?? ''}
					onUnlocked={() => {
						// The session store update drives the composer render.
					}}
				/>
			{/if}
		</div>
	</div>
{/if}
