<script lang="ts">
	import type { ChatMessageDto } from '$lib/api/chat';
	import MessageRow from './MessageRow.svelte';
	import { Loader2 } from 'lucide-svelte';

	interface Props {
		messages: ChatMessageDto[];
		loading: boolean;
		focusTarget: ChatMessageDto | null;
		onLoadMore: () => void;
		askAvailable?: boolean;
		communityId?: string;
	}

	let {
		messages,
		loading,
		focusTarget,
		onLoadMore,
		askAvailable = false,
		communityId = ''
	}: Props = $props();

	let container = $state<HTMLDivElement | null>(null);
	let scrollAnchor = $state<HTMLDivElement | null>(null);
	let prevMessageCount = $state(0);

	interface RowMeta {
		message: ChatMessageDto;
		showHeader: boolean;
		dateSeparator: string | null;
	}

	const GROUP_MINUTES = 5;

	function toLocalMidnight(iso: string): Date {
		const d = new Date(iso);
		d.setHours(0, 0, 0, 0);
		return d;
	}

	function formatDateSeparator(iso: string): string {
		const date = new Date(iso);
		if (Number.isNaN(date.getTime())) return '';
		const now = new Date();
		const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
		const yesterday = new Date(today);
		yesterday.setDate(yesterday.getDate() - 1);
		const messageDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());

		if (messageDay.getTime() === today.getTime()) return 'Today';
		if (messageDay.getTime() === yesterday.getTime()) return 'Yesterday';
		return new Intl.DateTimeFormat(undefined, {
			weekday: 'long',
			year: 'numeric',
			month: 'short',
			day: 'numeric'
		}).format(date);
	}

	function rows(messagesList: ChatMessageDto[]): RowMeta[] {
		const result: RowMeta[] = [];
		for (let i = 0; i < messagesList.length; i++) {
			const message = messagesList[i];
			const prev = messagesList[i - 1] ?? null;
			const currentDay = toLocalMidnight(message.event_created_at);
			const prevDay = prev ? toLocalMidnight(prev.event_created_at) : null;
			const dateSeparator =
				prevDay && currentDay.getTime() === prevDay.getTime()
					? null
					: formatDateSeparator(message.event_created_at);

			let showHeader = true;
			if (prev && !dateSeparator) {
				const sameAuthor = prev.author_pubkey === message.author_pubkey;
				const prevTime = new Date(prev.event_created_at).getTime();
				const currentTime = new Date(message.event_created_at).getTime();
				const minutesApart = Math.abs(currentTime - prevTime) / (1000 * 60);
				showHeader = !(sameAuthor && minutesApart <= GROUP_MINUTES);
			}

			result.push({ message, showHeader, dateSeparator });
		}
		return result;
	}

	const rowMetas = $derived(rows(messages));
	const hasMore = $derived(messages.length > 0);

	// Scroll the deep-linked message into view once it is rendered.
	$effect(() => {
		if (!focusTarget || !container) return;
		if (!messages.some((m) => m.message_id === focusTarget.message_id)) return;
		container
			.querySelector(`[data-message-id="${focusTarget.message_id}"]`)
			?.scrollIntoView({ behavior: 'smooth', block: 'center' });
	});

	// Preserve scroll position when older messages are prepended.
	$effect(() => {
		const currentCount = messages.length;
		if (container && scrollAnchor && currentCount > prevMessageCount && prevMessageCount > 0) {
			requestAnimationFrame(() => {
				if (!scrollAnchor || !container) return;
				const newTop = scrollAnchor.offsetTop;
				container.scrollTop = newTop;
			});
		}
		prevMessageCount = currentCount;
	});
</script>

<div
	bind:this={container}
	class="flex min-h-0 flex-1 flex-col overflow-y-auto"
	role="log"
	aria-live="polite"
	aria-label="Message timeline"
>
	{#if loading && messages.length === 0}
		<div class="flex flex-1 items-center justify-center">
			<Loader2 size={20} class="animate-spin text-base-content/40" aria-label="Loading messages" />
		</div>
	{:else if messages.length === 0}
		<div class="flex flex-1 flex-col items-center justify-center px-4 py-12 text-center">
			<p class="text-base-content/60">No messages yet</p>
			<p class="mt-1 text-sm text-base-content/50">Be the first to send something.</p>
		</div>
	{:else}
		<div class="py-2">
			<div class="px-4 pb-2">
				<button
					type="button"
					class="btn btn-ghost btn-sm w-full text-sm text-primary"
					disabled={loading}
					onclick={onLoadMore}
				>
					{#if loading}
						<Loader2 size={14} class="mr-1 animate-spin" />
						Loading older messages…
					{:else}
						Load earlier messages
					{/if}
				</button>
			</div>
			<div bind:this={scrollAnchor}></div>
			{#each rowMetas as { message, showHeader, dateSeparator } (message.event_id)}
				{#if dateSeparator}
					<div class="sticky top-2 z-10 my-3 flex justify-center">
						<span
							class="rounded-full bg-base-200 px-3 py-1 text-xs font-medium text-base-content/70 shadow-sm"
						>
							{dateSeparator}
						</span>
					</div>
				{/if}
				<MessageRow
					{message}
					{showHeader}
					{askAvailable}
					{communityId}
					isFocused={focusTarget?.message_id === message.message_id}
				/>
			{/each}
		</div>
	{/if}
</div>
