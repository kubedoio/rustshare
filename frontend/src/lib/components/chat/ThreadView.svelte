<script lang="ts">
	import type { NostrEvent } from '$lib/chat/nostr';
	import {
		parseThreadRefs,
		threadReplies,
		type DeletionInfo,
		type ReactionGroup,
		type ThreadParentRef
	} from '$lib/chat/conversations';
	import { REACTION_EMOJIS } from '$lib/chat/conversations';
	import { Reply, Trash2, X } from 'lucide-svelte';

	interface Props {
		rootId: string;
		/** Root message from the observed timeline, used until relay history lands. */
		rootSummary?: { authorPubkey: string; body: string | null; createdAt: string } | null;
		events: NostrEvent[];
		loading: boolean;
		error: boolean;
		viewerPubkey: string;
		reactions: Map<string, ReactionGroup[]>;
		deletedTargets: Map<string, DeletionInfo>;
		reactionsAvailable: boolean;
		canDelete: (authorPubkey: string) => 'self' | 'admin' | null;
		busyTarget: string | null;
		onClose: () => void;
		onReply: (parent: ThreadParentRef) => void;
		onReact: (targetId: string, emoji: string) => void;
		onDelete: (event: NostrEvent) => void;
	}

	let {
		rootId,
		rootSummary = null,
		events,
		loading,
		error,
		viewerPubkey,
		reactions,
		deletedTargets,
		reactionsAvailable,
		canDelete,
		busyTarget,
		onClose,
		onReply,
		onReact,
		onDelete
	}: Props = $props();

	const rootEvent = $derived(events.find((event) => event.id === rootId) ?? null);
	// Ancestry-filtered, oldest-first: the #e query also returns sibling
	// threads' events that happen to reference this root, so rendering must
	// group by resolved thread root, not by tag presence.
	const replies = $derived(threadReplies(events, rootId));

	function formatTime(secs: number): string {
		const date = new Date(secs * 1000);
		if (Number.isNaN(date.getTime())) return '';
		return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(date);
	}

	function short(pubkey: string): string {
		return pubkey.slice(0, 8);
	}
</script>

<div class="flex min-h-0 flex-1 flex-col border-l border-base-300 bg-base-100" aria-label="Thread">
	<div class="flex items-center justify-between border-b border-base-300 px-4 py-2">
		<h2 class="text-sm font-semibold">Thread</h2>
		<div class="flex items-center gap-2">
			<span class="text-xs text-base-content/50"
				>{replies.length} {replies.length === 1 ? 'reply' : 'replies'}</span
			>
			<button
				type="button"
				class="btn h-7 w-7 btn-ghost p-0 btn-xs"
				aria-label="Close thread"
				onclick={onClose}
			>
				<X size={14} />
			</button>
		</div>
	</div>

	<div
		class="flex min-h-0 flex-1 flex-col overflow-y-auto py-2"
		role="log"
		aria-label="Thread messages"
	>
		{#if loading && events.length === 0}
			<p class="px-4 py-6 text-sm text-base-content/60" role="status">Loading thread…</p>
		{:else if error && events.length === 0}
			<p class="px-4 py-6 text-sm text-error" role="alert">
				Thread history unavailable. Close the thread and retry when connected.
			</p>
		{:else if !rootEvent && !rootSummary}
			<p class="px-4 py-6 text-sm text-base-content/60">Thread unavailable.</p>
		{:else}
			{@const rootDeleted = deletedTargets.has(rootId)}
			<div class="px-4 py-1" data-event-id={rootId}>
				<div class="flex items-baseline gap-2">
					<span class="text-sm font-semibold">
						{rootEvent ? short(rootEvent.pubkey) : short(rootSummary?.authorPubkey ?? '')}
					</span>
					<span class="text-xs text-base-content/50">
						{rootEvent ? formatTime(rootEvent.created_at) : ''}
					</span>
				</div>
				{#if rootDeleted}
					<p class="text-sm text-base-content/50 italic">Message deleted.</p>
				{:else if rootEvent}
					<p class="text-sm whitespace-pre-wrap text-base-content">{rootEvent.content}</p>
				{:else}
					<p class="text-sm whitespace-pre-wrap text-base-content">{rootSummary?.body ?? ''}</p>
				{/if}
				{#if !rootDeleted}
					{@const rootReactions = reactions.get(rootId) ?? []}
					<div class="mt-1 flex flex-wrap items-center gap-1">
						{#each rootReactions as group (group.emoji)}
							<button
								type="button"
								class="badge gap-1 badge-sm {group.myEventId ? 'badge-primary' : 'badge-outline'}"
								aria-pressed={group.myEventId !== null}
								aria-label="Reaction {group.emoji}, {group.count}"
								disabled={!reactionsAvailable || busyTarget === rootId}
								title={group.authors.map(short).join(', ')}
								onclick={() => onReact(rootId, group.emoji)}
							>
								{group.emoji}
								{group.count}
							</button>
						{/each}
						{#if reactionsAvailable}
							{#each REACTION_EMOJIS.filter((emoji) => !rootReactions.some((g) => g.emoji === emoji)) as emoji (emoji)}
								<button
									type="button"
									class="badge badge-ghost badge-sm"
									aria-label="React {emoji}"
									disabled={busyTarget === rootId}
									onclick={() => onReact(rootId, emoji)}>{emoji}</button
								>
							{/each}
						{/if}
					</div>
				{/if}
			</div>

			{#each replies as event (event.id)}
				{@const refs = parseThreadRefs(event)}
				{@const deleted = deletedTargets.has(event.id)}
				<div class="ml-4 border-l-2 border-base-300 py-1 pr-4 pl-4" data-event-id={event.id}>
					<div class="flex items-baseline gap-2">
						<span class="text-sm font-semibold">{short(event.pubkey)}</span>
						<span class="text-xs text-base-content/50">{formatTime(event.created_at)}</span>
						{#if refs.parentId && refs.parentId !== refs.rootId}
							<span class="text-xs text-base-content/40">reply to {short(refs.parentId)}</span>
						{/if}
					</div>
					{#if deleted}
						<p class="text-sm text-base-content/50 italic">Message deleted.</p>
					{:else}
						<p class="text-sm whitespace-pre-wrap text-base-content">{event.content}</p>
					{/if}
					{#if !deleted}
						{@const eventReactions = reactions.get(event.id) ?? []}
						<div class="mt-1 flex flex-wrap items-center gap-1">
							{#each eventReactions as group (group.emoji)}
								<button
									type="button"
									class="badge gap-1 badge-sm {group.myEventId ? 'badge-primary' : 'badge-outline'}"
									aria-pressed={group.myEventId !== null}
									aria-label="Reaction {group.emoji}, {group.count}"
									disabled={!reactionsAvailable || busyTarget === event.id}
									title={group.authors.map(short).join(', ')}
									onclick={() => onReact(event.id, group.emoji)}
								>
									{group.emoji}
									{group.count}
								</button>
							{/each}
							{#if reactionsAvailable}
								{#each REACTION_EMOJIS.filter((emoji) => !eventReactions.some((g) => g.emoji === emoji)) as emoji (emoji)}
									<button
										type="button"
										class="badge badge-ghost badge-sm"
										aria-label="React {emoji}"
										disabled={busyTarget === event.id}
										onclick={() => onReact(event.id, emoji)}>{emoji}</button
									>
								{/each}
							{/if}
						</div>
					{/if}
					<div class="mt-1 flex items-center gap-3 text-xs">
						<button
							type="button"
							class="inline-flex items-center gap-1 text-base-content/60 hover:underline"
							aria-label="Reply to {short(event.pubkey)}"
							onclick={() => onReply({ eventId: event.id, rootId: refs.rootId ?? rootId })}
						>
							<Reply size={12} /> Reply
						</button>
						{#if canDelete(event.pubkey) && !deleted}
							<button
								type="button"
								class="inline-flex items-center gap-1 text-error hover:underline"
								aria-label="Delete message {short(event.id)}"
								disabled={busyTarget === event.id}
								onclick={() => onDelete(event)}
							>
								<Trash2 size={12} /> Delete
							</button>
						{/if}
					</div>
				</div>
			{/each}
		{/if}
	</div>
</div>
