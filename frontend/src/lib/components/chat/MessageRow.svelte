<script lang="ts">
	import type { ChatAttachmentDto, ChatMessageDto } from '$lib/api/chat';
	import { openChatAttachment } from '$lib/api/chat';
	import { apiClient } from '$lib/api/client';
	import { copyToClipboard } from '$lib/utils/clipboard';
	import { MoreHorizontal, Sparkles, Link, FileText } from 'lucide-svelte';

	interface Props {
		message: ChatMessageDto;
		showHeader: boolean;
		askAvailable: boolean;
		communityId?: string;
		isFocused?: boolean;
	}

	let { message, showHeader, askAvailable, communityId = '', isFocused = false }: Props = $props();

	let menuOpen = $state(false);

	const displayName = $derived(message.author?.display_name ?? 'Unknown Buzz user');
	const avatarUrl = $derived(
		message.author?.avatar_url ? `${apiClient.getBaseURL()}${message.author.avatar_url}` : null
	);
	const pubkeyShort = $derived(message.author_pubkey.slice(0, 8));
	const initials = $derived(getInitials(displayName));

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
		const base = window.location.href.split('?')[0].split('#')[0];
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
</script>

<svelte:window onclick={() => menuOpen && closeMenu()} />

<div
	class="group relative px-4 py-1 hover:bg-base-200/40 focus-within:bg-base-200/40 {isFocused
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

			{#if message.body != null}
				<div class="whitespace-pre-wrap text-sm text-base-content">{message.body}</div>
			{:else}
				<div class="text-sm text-base-content/50 italic">Content unavailable in Elembra.</div>
			{/if}

			{#if message.attachments.length > 0}
				<div class="mt-1 flex flex-wrap gap-2">
					{#each message.attachments as attachment (attachment.application + attachment.resourceType + attachment.resourceId + (attachment.version ?? ''))}
						<button
							type="button"
							class="badge badge-outline badge-sm inline-flex items-center gap-1 text-xs"
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
		</div>
	</div>

	<!-- Message actions -->
	<div
		class="absolute top-1 right-3 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
	>
		<div class="relative">
			<button
				type="button"
				class="btn btn-ghost btn-xs h-7 w-7 rounded-lg p-0"
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
				</div>
			{/if}
		</div>
	</div>
</div>
