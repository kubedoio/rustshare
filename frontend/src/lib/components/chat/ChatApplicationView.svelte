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
	import { getChatCapabilities, type ChatChannelState } from '$lib/chat/capabilities';
	import {
		listVisibleChannels,
		isChannelId,
		type ChannelDirectoryResult
	} from '$lib/chat/channels';
	import {
		canDeleteMessage,
		deleteMessage,
		fetchConversationOverlay,
		fetchThreadHistory,
		parseDeletionTargets,
		parseReactions,
		removeReaction,
		retryConversationMutation,
		sendReaction,
		type ConversationMutationInput,
		type ConversationMutationResult,
		type ThreadParentRef
	} from '$lib/chat/conversations';
	import type { NostrEvent } from '$lib/chat/nostr';
	import ThreadView from './ThreadView.svelte';

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
	// channels the caller may not see never reach this query's result. The
	// query key carries the bound identity: the cached directory is scoped per
	// identity, so one identity can never be served another identity's
	// private-channel entries from the TanStack cache.
	const directoryQuery = createQuery<ChannelDirectoryResult>({
		queryKey: ['chat-channel-directory', null, null, null],
		queryFn: () => Promise.resolve({ ok: true, channels: [] }),
		enabled: false
	});

	let selectedChannelId = $state<string | null>(null);
	let focusedMessageId = $state<string | null>(null);
	let cursor = $state<string | null>(null);

	// Deep links: /apps/chat?channel=<id>&message=<id>. The channel param is
	// honored only when it is a relay-assigned channel UUID; anything else is
	// ignored rather than trusted as a selection.
	$effect(() => {
		const params = $page.url.searchParams;
		const channel = params.get('channel');
		const message = params.get('message');
		if (channel && isChannelId(channel)) selectedChannelId = channel;
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
		// The directory cache is scoped to the bound chat identity: switching
		// identities must never surface the previous identity's private entries.
		const identity = $chatSessionStore.state === 'unlocked' ? $chatSessionStore.pubkey : null;
		directoryQuery.setOptions({
			queryKey: [
				'chat-channel-directory',
				status?.mapping?.relay_url ?? null,
				status?.mapping?.relay_pubkey ?? null,
				identity
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
			// Keep the previous snapshot only while the identity is unchanged;
			// a fresh identity must never be shown the previous identity's data.
			placeholderData: (previous, previousQuery) =>
				previousQuery?.queryKey[3] === identity ? previous : undefined
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
			// The pinned fork always emits the channel type as the 39000 `t`
			// tag, so channelType is null only for metadata from other relays;
			// 'stream' is the fork's ingest-time default (ingest.rs).
			channel_kind: entry.channelType ?? 'stream',
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

	// Relay-backed conversation overlay (kinds 5/7/9005/40099): reactions and
	// deletion signals for the selected channel. The backend observation path
	// only accepts chat-message kinds (1/9/40002), so this state is read from
	// the relay exactly like the roster query — access control happens at the
	// relay, so a lost membership fails closed here.
	const conversationQuery = createQuery<Awaited<ReturnType<typeof fetchConversationOverlay>>>({
		queryKey: ['chat-conversation-overlay', null, null, null, null],
		queryFn: () => Promise.resolve({ ok: true as const, events: [] }),
		enabled: false
	});

	const threadQuery = createQuery<Awaited<ReturnType<typeof fetchThreadHistory>>>({
		queryKey: ['chat-thread-history', null, null, null, null],
		queryFn: () => Promise.resolve({ ok: true as const, events: [] }),
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

	const conversationReadsEnabled = $derived(
		capabilities.reactions.available &&
			capabilities.deleteMessage.available &&
			status?.mapping?.relay_url != null &&
			status.mapping.relay_pubkey != null &&
			selectedChannelId != null &&
			isChannelId(selectedChannelId) &&
			$chatSessionStore.state === 'unlocked'
	);

	$effect(() => {
		const mapping = status?.mapping;
		const identity = $chatSessionStore.state === 'unlocked' ? $chatSessionStore.pubkey : null;
		conversationQuery.setOptions({
			queryKey: [
				'chat-conversation-overlay',
				mapping?.relay_url ?? null,
				mapping?.relay_pubkey ?? null,
				selectedChannelId,
				identity
			],
			queryFn: () => {
				const key = getSigningKey();
				if (!mapping?.relay_url || !mapping.relay_pubkey || !key || !selectedChannelId)
					return Promise.resolve({ ok: true as const, events: [] });
				return fetchConversationOverlay(
					{ relayUrl: mapping.relay_url, secretKey: key, relayPubkey: mapping.relay_pubkey },
					selectedChannelId
				);
			},
			enabled: conversationReadsEnabled,
			staleTime: 15_000,
			placeholderData: (previous, previousQuery) =>
				previousQuery?.queryKey[4] === identity && previousQuery?.queryKey[3] === selectedChannelId
					? previous
					: undefined
		});
		threadQuery.setOptions({
			queryKey: [
				'chat-thread-history',
				mapping?.relay_url ?? null,
				mapping?.relay_pubkey ?? null,
				threadRootId,
				identity
			],
			queryFn: () => {
				const key = getSigningKey();
				if (!mapping?.relay_url || !mapping.relay_pubkey || !key || !threadRootId)
					return Promise.resolve({ ok: true as const, events: [] });
				return fetchThreadHistory(
					{ relayUrl: mapping.relay_url, secretKey: key, relayPubkey: mapping.relay_pubkey },
					threadRootId
				);
			},
			enabled: conversationReadsEnabled && threadRootId !== null,
			staleTime: 15_000,
			placeholderData: (previous, previousQuery) =>
				previousQuery?.queryKey[4] === identity && previousQuery?.queryKey[3] === threadRootId
					? previous
					: undefined
		});
	});

	const viewerPubkey = $derived(
		$chatSessionStore.state === 'unlocked' ? $chatSessionStore.pubkey : null
	);
	const overlayEvents = $derived($conversationQuery.data?.ok ? $conversationQuery.data.events : []);
	const reactionsByTarget = $derived(parseReactions(overlayEvents, viewerPubkey));
	const deletedTargets = $derived(
		parseDeletionTargets(overlayEvents, status?.mapping?.relay_pubkey ?? '')
	);
	// Reply badges come from the observed timeline (the projection carries
	// thread_root_id), so they appear as Elembra observes each reply.
	const replyCounts = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const message of accumulatedMessages) {
			if (message.thread_root_id && message.thread_root_id !== message.event_id) {
				counts.set(message.thread_root_id, (counts.get(message.thread_root_id) ?? 0) + 1);
			}
		}
		return counts;
	});
	const threadEvents = $derived($threadQuery.data?.ok ? $threadQuery.data.events : []);
	const deleteModeFor = (authorPubkey: string): 'self' | 'admin' | null => {
		if (!viewerPubkey || !capabilities.deleteMessage.available) return null;
		return canDeleteMessage(channelState?.role ?? null, authorPubkey, viewerPubkey);
	};

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
			if (conversationReadsEnabled) {
				conversationQuery.refetch();
				if (threadRootId) threadQuery.refetch();
			}
		}, 15_000);
		return () => clearInterval(interval);
	});

	let pendingEventId = $state<string | null>(null);
	let syncState = $state<'idle' | 'waiting' | 'observed' | 'warning'>('idle');
	let accumulatedMessages = $state<ChatMessageDto[]>([]);
	// Conversation state (PR #481): reply composer target, open thread root, and
	// the relay-confirmed channel state used to role-gate deletion.
	let channelState = $state<ChatChannelState | null>(null);
	let threadRootId = $state<string | null>(null);
	let replyTarget = $state<ThreadParentRef | null>(null);
	let conversationBusy = $state(false);
	let busyTarget = $state<string | null>(null);
	let conversationError = $state('');
	let conversationNotice = $state('');
	let pendingConversationRetry = $state<{
		event: NostrEvent;
		confirm: () => Promise<boolean>;
		success: string;
	} | null>(null);
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
	// Conversation state is scoped to the channel too: replies, the open
	// thread, and any stale operation status must not leak across channels.
	$effect(() => {
		if (selectedChannelId !== null) {
			accumulatedMessages = [];
			pendingEventId = null;
			syncState = 'idle';
			relayError = '';
			threadRootId = null;
			replyTarget = null;
			conversationError = '';
			conversationNotice = '';
			pendingConversationRetry = null;
		}
	});

	// Losing posting eligibility (role change, removal from a private channel,
	// offline) must hide thread state and drop any pending reply: the composer
	// is default-denied in that state, and leaving a stale thread view open
	// would show history the current role may no longer act on.
	$effect(() => {
		if (!postingAllowed) {
			threadRootId = null;
			replyTarget = null;
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

	function conversationInput(): ConversationMutationInput | null {
		const mapping = status?.mapping;
		const secretKey = getSigningKey();
		if (!mapping?.relay_url || !mapping.relay_pubkey || !secretKey) {
			conversationError = 'Chat identity is locked.';
			return null;
		}
		return { relayUrl: mapping.relay_url, secretKey, relayPubkey: mapping.relay_pubkey };
	}

	function applyConversationResult(result: ConversationMutationResult, success: string): void {
		conversationNotice = '';
		if (result.status === 'confirmed') {
			pendingConversationRetry = null;
			conversationError = '';
			conversationNotice = success;
			void conversationQuery.refetch();
			if (threadRootId) void threadQuery.refetch();
		} else if (result.status === 'unconfirmed') {
			conversationError =
				'The relay accepted the request but the change is not visible yet. It may still apply; refresh to re-check.';
		} else if (result.status === 'uncertain') {
			conversationError =
				'Relay acknowledgement missing. The operation may already have been accepted — retry only the same signed request.';
		} else if (result.status === 'rejected') {
			const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
			conversationError = `Relay rejected the operation${detail}`;
		} else {
			conversationError = 'Relay unreachable. Retry when connected.';
		}
	}

	async function runConversationOp(
		operation: (input: ConversationMutationInput) => Promise<ConversationMutationResult>,
		success: string,
		targetId: string | null = null
	): Promise<ConversationMutationResult | null> {
		if (conversationBusy) return null;
		const input = conversationInput();
		if (!input) return null;
		conversationBusy = true;
		busyTarget = targetId;
		conversationError = '';
		conversationNotice = '';
		try {
			let confirm: (() => Promise<boolean>) | null = null;
			const result = await operation({ ...input, onConfirm: (c) => (confirm = c) });
			if (!getSigningKey()) return null;
			applyConversationResult(result, success);
			if (result.status === 'uncertain') {
				pendingConversationRetry = {
					event: result.event,
					confirm: confirm ?? (() => Promise.resolve(true)),
					success
				};
			}
			return result;
		} finally {
			conversationBusy = false;
			busyTarget = null;
		}
	}

	async function retryConversationOp(): Promise<void> {
		if (!pendingConversationRetry || conversationBusy) return;
		const input = conversationInput();
		if (!input) return;
		const pending = pendingConversationRetry;
		conversationBusy = true;
		conversationError = '';
		try {
			const result = await retryConversationMutation(input, pending.event, pending.confirm);
			if (!getSigningKey()) return;
			applyConversationResult(result, pending.success);
			if (result.status !== 'uncertain') pendingConversationRetry = null;
		} finally {
			conversationBusy = false;
		}
	}

	async function toggleReaction(targetId: string, emoji: string): Promise<void> {
		const channelId = selectedChannelId;
		if (!channelId || !capabilities.reactions.available) return;
		const mine = reactionsByTarget.get(targetId)?.find((group) => group.emoji === emoji)?.myEventId;
		if (mine) {
			await runConversationOp(
				(input) => removeReaction(input, channelId, { eventId: mine, targetId }),
				'Reaction removed.',
				targetId
			);
		} else {
			await runConversationOp(
				(input) => sendReaction(input, channelId, targetId, emoji),
				'Reaction added.',
				targetId
			);
		}
	}

	async function handleDeleteEvent(
		targetEventId: string,
		authorPubkey: string,
		authorShort: string
	): Promise<void> {
		const channelId = selectedChannelId;
		if (!channelId || !viewerPubkey) return;
		const mode = canDeleteMessage(channelState?.role ?? null, authorPubkey, viewerPubkey);
		if (!mode) return;
		const confirmed = window.confirm(
			mode === 'self'
				? 'Delete this message for everyone? This cannot be undone.'
				: `Delete ${authorShort}'s message as channel ${channelState?.role}? The relay records the deletion.`
		);
		if (!confirmed) return;
		await runConversationOp(
			(input) => deleteMessage(input, channelId, targetEventId, mode),
			'Message deleted.',
			targetEventId
		);
	}

	function openThread(message: ChatMessageDto): void {
		threadRootId = message.thread_root_id ?? message.event_id;
		replyTarget = null;
	}

	function closeThread(): void {
		threadRootId = null;
		replyTarget = null;
	}

	function handleReplyTo(message: ChatMessageDto): void {
		replyTarget = {
			eventId: message.event_id,
			rootId: message.thread_root_id ?? message.event_id
		};
		textareaFocusQueued = true;
	}

	function handleReplyInThread(parent: ThreadParentRef): void {
		replyTarget = parent;
		textareaFocusQueued = true;
	}

	let textareaFocusQueued = $state(false);
	$effect(() => {
		if (!textareaFocusQueued) return;
		textareaFocusQueued = false;
		// The composer remounts only per channel; focusing is best-effort.
		document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Message text"]')?.focus();
	});

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
						onStateChange={(state) => (channelState = state)}
						onChanged={() => directoryQuery.refetch()}
					/>
				{/key}
			{/if}

			{#if syncState !== 'idle' || cursor || relayError || conversationError || conversationNotice || conversationBusy}
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
						{#if conversationBusy}
							<span class="text-base-content/60" role="status">Updating conversation…</span>
						{/if}
						{#if conversationNotice}
							<span class="text-success" role="status">{conversationNotice}</span>
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
					{#if conversationError}
						<p class="mt-1 text-sm text-error" role="alert">{conversationError}</p>
					{/if}
					{#if pendingConversationRetry && !conversationBusy}
						<div role="status" class="mt-1 text-warning">
							<p>The operation’s acceptance is uncertain. Retry only the same signed request.</p>
							<button type="button" class="btn btn-sm" onclick={() => void retryConversationOp()}
								>Retry same request</button
							>
						</div>
					{/if}
				</div>
			{/if}

			{#if threadRootId && postingAllowed}
				{@const rootMessage = accumulatedMessages.find((m) => m.event_id === threadRootId) ?? null}
				<ThreadView
					rootId={threadRootId}
					rootSummary={rootMessage
						? {
								authorPubkey: rootMessage.author_pubkey,
								body: rootMessage.body,
								createdAt: rootMessage.event_created_at
							}
						: null}
					events={threadEvents}
					loading={$threadQuery.isLoading}
					error={$threadQuery.isError || $threadQuery.data?.ok === false}
					viewerPubkey={viewerPubkey ?? ''}
					reactions={reactionsByTarget}
					{deletedTargets}
					reactionsAvailable={capabilities.reactions.available}
					canDelete={(author) => deleteModeFor(author)}
					{busyTarget}
					onClose={closeThread}
					onReply={handleReplyInThread}
					onReact={(targetId, emoji) => void toggleReaction(targetId, emoji)}
					onDelete={(event) =>
						void handleDeleteEvent(event.id, event.pubkey, event.pubkey.slice(0, 8))}
				/>
			{:else}
				<MessageTimeline
					messages={accumulatedMessages}
					loading={$messagesQuery.isLoading}
					{focusTarget}
					hasMore={hasMoreMessages}
					askAvailable={status.ask_available}
					communityId={status.mapping?.community_id ?? ''}
					reactionsAvailable={capabilities.reactions.available}
					{reactionsByTarget}
					{deletedTargets}
					deleteModeFor={(author) => deleteModeFor(author)}
					threadAvailable={capabilities.threads.available}
					{replyCounts}
					{busyTarget}
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
					onReply={handleReplyTo}
					onOpenThread={openThread}
					onReact={(targetId, emoji) => void toggleReaction(targetId, emoji)}
					onDelete={(message) =>
						void handleDeleteEvent(
							message.event_id,
							message.author_pubkey,
							message.author_pubkey.slice(0, 8)
						)}
				/>
			{/if}
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
						replyTarget={threadRootId && postingAllowed
							? (replyTarget ?? { eventId: threadRootId, rootId: threadRootId })
							: null}
						onCancelReply={() => {
							// In a thread the composer defaults to the thread root: cancel
							// an explicit reply target first, then close the thread.
							if (replyTarget && replyTarget.eventId !== threadRootId) replyTarget = null;
							else closeThread();
						}}
						onSendFailure={handleSendFailure}
						onSent={(eventId: string) => {
							cursor = null;
							pendingEventId = eventId;
							syncState = 'waiting';
							if (threadRootId) void threadQuery.refetch();
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
