<script lang="ts">
	import type { ChatAttachmentDto, ChatMessageDto } from '$lib/api/chat';
	import { openChatAttachment } from '$lib/api/chat';
	import { apiClient } from '$lib/api/client';
	import { copyToClipboard } from '$lib/utils/clipboard';
	import { REACTION_EMOJIS, type DeletionInfo, type ReactionGroup } from '$lib/chat/conversations';
	import {
		MoreHorizontal,
		Sparkles,
		Link,
		FileText,
		Reply,
		MessageSquare,
		Smile,
		Trash2
	} from 'lucide-svelte';

	interface Props {
		message: ChatMessageDto;
		showHeader: boolean;
		askAvailable: boolean;
		communityId?: string;
		isFocused?: boolean;
		reactionsAvailable?: boolean;
		reactions?: ReactionGroup[];
		deletedInfo?: DeletionInfo | null;
		deleteMode?: 'self' | 'admin' | null;
		threadAvailable?: boolean;
		replyCount?: number;
		busy?: boolean;
		onReply?: () => void;
		onOpenThread?: () => void;
		onReact?: (emoji: string) => void;
		onDelete?: () => void;
	}

	let {
		message,
		showHeader,
		askAvailable,
		communityId = '',
		isFocused = false,
		reactionsAvailable = false,
		reactions = [],
		deletedInfo = null,
		deleteMode = null,
		threadAvailable = false,
		replyCount = 0,
		busy = false,
		onReply = () => {},
		onOpenThread = () => {},
		onReact = () => {},
		onDelete = () => {}
	}: Props = $props();

	let menuOpen = $state(false);
	let reactionPickerOpen = $state(false);

	const displayName = $derived(message.author?.display_name ?? 'Unknown Buzz user');
	const avatarUrl = $derived(
		message.author?.avatar_url
			? new URL(message.author.avatar_url, apiClient.getBaseURL()).href
			: null
	);
	const pubkeyShort = $derived(message.author_pubkey.slice(0, 8));
	const initials = $derived(getInitials(displayName));
	const deleted = $derived(deletedInfo !== null);

	function getInitials(name: string): string {
		const trimmed = name.trim();
		if (!trimmed) return '??';
		const parts = trimmed.split(/\s+/);
		if (parts.length === 1) return trimmed.charAt(0).toUpperCase();
		return (parts[0].charAt(0) + parts[parts.length - 1].charAt(0)).toUpperCase();
	}

	function formatTime(iso: string): string {
		const date = new Date(iso);
		if (Number.isNaN(date.getTime())) return '';
		return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(date);
	}

	function askHref(): string {
		const params = new URLSearchParams({ scope: 'chat' });
		if (communityId) params.set('communityId', communityId);
		params.set('channelId', message.channel_id);
		params.set('messageId', message.message_id);
		return `/ask?${params.toString()}`;
	}

	function copyLink(): void {
		const base =
			typeof window !== 'undefined' ? window.location.href.split('?')[0].split('#')[0] : '';
		const link = `${base}?channel=${encodeURIComponent(message.channel_id)}&message=${encodeURIComponent(message.message_id)}`;
		copyToClipboard(link);
	}

	async function openAttachment(attachment: ChatAttachmentDto, event: Event): Promise<void> {
		event.stopPropagation();
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

	function toggleMenu(event: Event): void {
		event.stopPropagation();
		menuOpen = !menuOpen;
	}

	function closeMenu(): void {
		menuOpen = false;
	}

	function toggleReactionPicker(event: Event): void {
		event.stopPropagation();
		reactionPickerOpen = !reactionPickerOpen;
	}
</script>

<svelte:window
	onclick={() => {
		if (menuOpen) closeMenu();
		if (reactionPickerOpen) reactionPickerOpen = false;
	}}
/>

<div
	class="group relative px-4 py-1 focus-within:bg-base-200/40 hover:bg-base-200/40 {isFocused
		? 'bg-primary/10 ring-1 ring-primary/20'
		: ''}"
	data-message-id={message.message_id}
	id={message.message_id}
>
	<div class="flex gap-3 {showHeader ? 'pt-2' : ''}">
		{#if showHeader}
			<div class="relative h-9 w-9 shrink-0 overflow-hidden rounded-lg bg-base-200">
				{#if avatarUrl}
					<img src={avatarUrl} alt="" class="h-full w-full object-cover" loading="lazy" />
				{:else}
					<div
						class="flex h-full w-full items-center justify-center text-xs font-semibold text-base-content/70"
					>
						{initials}
					</div>
				{/if}
			</div>
		{:else}
			<div class="h-9 w-9 shrink-0"></div>
		{/if}

		<div class="min-w-0 flex-1">
			{#if showHeader}
				<div class="flex items-baseline gap-2">
					<span class="text-sm font-semibold text-base-content">{displayName}</span>
					<span class="font-mono text-xs text-base-content/40" title={message.author_pubkey}>
						{pubkeyShort}
					</span>
					<time
						class="text-xs text-base-content/50"
						datetime={message.event_created_at}
						title={message.event_created_at}
					>
						{formatTime(message.event_created_at)}
					</time>
				</div>
			{/if}

			{#if deleted}
				<div class="text-sm text-base-content/50 italic">Message deleted.</div>
			{:else if message.body != null}
				<div class="text-sm whitespace-pre-wrap text-base-content">{message.body}</div>
			{:else}
				<div class="text-sm text-base-content/50 italic">Content unavailable in Elembra.</div>
			{/if}

			{#if !deleted && message.attachments.length > 0}
				<div class="mt-1 flex flex-wrap gap-2">
					{#each message.attachments as attachment (attachment.application + attachment.resourceType + attachment.resourceId + (attachment.version ?? ''))}
						<button
							type="button"
							class="badge inline-flex items-center gap-1 badge-outline text-xs badge-sm"
							aria-label="Open attachment"
							title="Open attachment"
							onclick={(e) => openAttachment(attachment, e)}
						>
							<FileText size={10} />
							{attachment.resourceId}
						</button>
					{/each}
				</div>
			{/if}

			{#if !deleted && (reactions.length > 0 || reactionsAvailable)}
				<div class="mt-1 flex flex-wrap items-center gap-1">
					{#each reactions as group (group.emoji)}
						<button
							type="button"
							class="badge gap-1 badge-sm {group.myEventId ? 'badge-primary' : 'badge-outline'}"
							aria-pressed={group.myEventId !== null}
							aria-label="Reaction {group.emoji}, {group.count}"
							title={group.authors.map((a) => a.slice(0, 8)).join(', ')}
							disabled={busy}
							onclick={() => onReact(group.emoji)}
						>
							{group.emoji}
							{group.count}
						</button>
					{/each}
					{#if reactionsAvailable}
						<span class="relative">
							<button
								type="button"
								class="badge inline-flex items-center gap-1 badge-ghost badge-sm"
								aria-label="Add reaction"
								aria-expanded={reactionPickerOpen}
								disabled={busy}
								onclick={toggleReactionPicker}
							>
								<Smile size={12} />
							</button>
							{#if reactionPickerOpen}
								<span
									class="absolute top-full left-0 z-20 mt-1 flex gap-1 rounded-lg border border-base-300 bg-base-100 p-1 shadow-xl shadow-black/10"
									role="menu"
								>
									{#each REACTION_EMOJIS as emoji (emoji)}
										<button
											type="button"
											role="menuitem"
											class="btn h-7 w-7 btn-ghost p-0 btn-xs"
											aria-label="React {emoji}"
											disabled={busy}
											onclick={() => {
												reactionPickerOpen = false;
												onReact(emoji);
											}}>{emoji}</button
										>
									{/each}
								</span>
							{/if}
						</span>
					{/if}
				</div>
			{/if}
		</div>
	</div>

	<!-- Message actions -->
	{#if !deleted}
		<div
			class="absolute top-1 right-3 flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
		>
			{#if threadAvailable}
				<button
					type="button"
					class="btn h-7 w-7 rounded-lg btn-ghost p-0 btn-xs"
					aria-label="Reply to message"
					title="Reply"
					disabled={busy}
					onclick={onReply}
				>
					<Reply size={14} />
				</button>
				{#if replyCount > 0}
					<button
						type="button"
						class="btn h-7 rounded-lg btn-ghost p-0 px-1.5 btn-xs"
						aria-label="View thread, {replyCount} {replyCount === 1 ? 'reply' : 'replies'}"
						title="View thread"
						onclick={onOpenThread}
					>
						<MessageSquare size={14} />
						{replyCount}
					</button>
				{/if}
			{/if}
			<div class="relative">
				<button
					type="button"
					class="btn h-7 w-7 rounded-lg btn-ghost p-0 btn-xs"
					aria-label="Message actions"
					aria-expanded={menuOpen}
					onclick={toggleMenu}
				>
					<MoreHorizontal size={14} />
				</button>

				{#if menuOpen}
					<div
						class="absolute top-full right-0 z-20 mt-1 w-52 rounded-lg border border-base-300 bg-base-100 py-1 shadow-xl shadow-black/10"
						role="menu"
					>
						{#if askAvailable}
							<a
								href={askHref()}
								role="menuitem"
								class="flex items-center gap-2 px-3 py-2 text-sm hover:bg-base-200"
								onclick={closeMenu}
							>
								<Sparkles size={14} class="text-base-content/60" />
								Ask Elembra about this
							</a>
						{:else}
							<span
								role="menuitem"
								class="tooltip flex cursor-not-allowed items-center gap-2 px-3 py-2 text-sm text-base-content/40"
								data-tip="Ask Elembra is unavailable"
							>
								<Sparkles size={14} />
								Ask Elembra about this
							</span>
						{/if}
						<button
							type="button"
							role="menuitem"
							class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
							onclick={() => {
								copyLink();
								closeMenu();
							}}
						>
							<Link size={14} class="text-base-content/60" />
							Copy message link
						</button>
						{#if deleteMode}
							<button
								type="button"
								role="menuitem"
								class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-error hover:bg-base-200"
								onclick={() => {
									closeMenu();
									onDelete();
								}}
							>
								<Trash2 size={14} />
								{deleteMode === 'self' ? 'Delete message' : 'Delete message (admin)'}
							</button>
						{/if}
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>
