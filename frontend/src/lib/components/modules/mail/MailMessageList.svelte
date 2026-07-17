<script lang="ts">
	import {
		Paperclip,
		RefreshCw,
		Archive,
		Trash2,
		Trash,
		HardDriveDownload,
		Inbox
	} from 'lucide-svelte';
	import type { MailListItem } from './mail-types';
	import { formatMailDate, sourceBadge } from './mail-types';

	let {
		title,
		items,
		initialLoading,
		refreshing,
		error,
		searchActive,
		selectedKey,
		checkedUids,
		archiveAvailable = true,
		trashAvailable = true,
		hasMore,
		loadingMore,
		onOpen,
		onToggleCheck,
		onCheckAll,
		onClearChecks,
		onImportSelected,
		onArchiveSelected,
		onTrashSelected,
		onDeleteSelected,
		onLoadMore,
		onRetry
	}: {
		title: string;
		items: MailListItem[];
		initialLoading: boolean;
		refreshing: boolean;
		error: string | null;
		searchActive: boolean;
		selectedKey: string | null;
		checkedUids: number[];
		archiveAvailable?: boolean;
		trashAvailable?: boolean;
		hasMore: boolean;
		loadingMore: boolean;
		onOpen: (item: MailListItem) => void;
		onToggleCheck: (uid: number) => void;
		onCheckAll: () => void;
		onClearChecks: () => void;
		onImportSelected: () => void;
		onArchiveSelected: () => void;
		onTrashSelected: () => void;
		onDeleteSelected: () => void;
		onLoadMore: () => void;
		onRetry: () => void;
	} = $props();

	let imapUids = $derived(
		items
			.filter((item) => item.kind === 'imap')
			.map((item) => (item.kind === 'imap' ? item.uid : 0))
	);
	let allChecked = $derived(
		imapUids.length > 0 && imapUids.every((uid) => checkedUids.includes(uid))
	);
	let checkedCount = $derived(checkedUids.length);

	function itemKey(item: MailListItem): string {
		return item.kind === 'imap' ? `imap:${item.uid}` : `stored:${item.id}`;
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<!-- List header -->
	<div class="flex min-h-9 items-center gap-2 border-b border-[var(--rs-border)] px-3 py-1.5">
		{#if imapUids.length > 0}
			<input
				type="checkbox"
				class="checkbox checkbox-xs"
				checked={allChecked}
				aria-label="Select all visible messages"
				onchange={() => (allChecked ? onClearChecks() : onCheckAll())}
			/>
		{/if}
		<h2 class="min-w-0 flex-1 truncate text-xs font-semibold text-base-content/70">
			{title}
			{#if refreshing}
				<RefreshCw size={11} class="ml-1 inline animate-spin text-base-content/40" />
			{/if}
		</h2>
		{#if checkedCount > 0}
			<div
				class="flex items-center gap-0.5"
				role="toolbar"
				aria-label="Actions for selected messages"
			>
				<button
					type="button"
					class="btn btn-xs btn-primary gap-1"
					title="Import selected into RustShare"
					onclick={onImportSelected}
				>
					<HardDriveDownload size={12} /> Import {checkedCount}
				</button>
				<button
					type="button"
					class="btn btn-xs btn-ghost btn-square"
					disabled={!archiveAvailable}
					title={archiveAvailable ? 'Archive selected' : 'No archive folder is configured'}
					aria-label={archiveAvailable ? 'Archive selected' : 'No archive folder is configured'}
					onclick={onArchiveSelected}
				>
					<Archive size={13} />
				</button>
				<button
					type="button"
					class="btn btn-xs btn-ghost btn-square"
					disabled={!trashAvailable}
					title={trashAvailable ? 'Move selected to trash' : 'No trash folder is configured'}
					aria-label={trashAvailable ? 'Move selected to trash' : 'No trash folder is configured'}
					onclick={onTrashSelected}
				>
					<Trash2 size={13} />
				</button>
				<button
					type="button"
					class="btn btn-xs btn-ghost btn-square text-error"
					title="Delete selected permanently"
					aria-label="Delete selected permanently"
					onclick={onDeleteSelected}
				>
					<Trash size={13} />
				</button>
			</div>
		{/if}
	</div>

	<!-- Rows -->
	<div class="min-h-0 flex-1 overflow-y-auto" role="list" aria-label="Messages">
		{#if initialLoading}
			<div class="flex flex-col" aria-label="Loading messages">
				{#each Array(8) as _, i}
					<div class="flex flex-col gap-1.5 border-b border-[var(--rs-border)] px-3 py-2.5">
						<div class="skeleton h-3.5 w-2/5" class:opacity-70={i > 3}></div>
						<div class="skeleton h-3 w-4/5" class:opacity-70={i > 3}></div>
					</div>
				{/each}
			</div>
		{:else if error}
			<div class="flex flex-col items-center px-4 py-10 text-center" role="alert">
				<p class="text-sm font-semibold text-base-content">Messages could not be loaded.</p>
				<p class="mt-1 max-w-56 truncate text-xs text-base-content/55" title={error}>{error}</p>
				<button type="button" class="btn btn-xs btn-outline mt-3 gap-1" onclick={onRetry}>
					<RefreshCw size={11} /> Retry
				</button>
			</div>
		{:else if items.length === 0}
			<div class="flex flex-col items-center px-4 py-10 text-center">
				<Inbox size={22} class="text-base-content/25" />
				<p class="mt-2 text-sm font-medium text-base-content/70">
					{searchActive ? 'No messages match this search' : 'This folder is empty'}
				</p>
				{#if searchActive}
					<p class="mt-0.5 text-xs text-base-content/50">Try a different search term.</p>
				{/if}
			</div>
		{:else}
			{#each items as item (itemKey(item))}
				{@const key = itemKey(item)}
				{@const isImap = item.kind === 'imap'}
				{@const message = item.message}
				{@const unread = message.is_seen === false}
				<div
					class="group relative flex w-full items-start gap-2 border-b border-[var(--rs-border)] text-left transition-colors
						{selectedKey === key ? 'bg-brand-500/8' : 'hover:bg-base-200/70'}"
					role="listitem"
				>
					{#if isImap}
						<input
							type="checkbox"
							class="checkbox checkbox-xs mt-3 ml-2 shrink-0"
							checked={checkedUids.includes(item.kind === 'imap' ? item.uid : 0)}
							aria-label="Select message {message.subject || '(no subject)'}"
							onchange={() => item.kind === 'imap' && onToggleCheck(item.uid)}
						/>
					{/if}
					<button
						type="button"
						class="min-w-0 flex-1 px-3 py-2.5 text-left"
						aria-current={selectedKey === key ? 'true' : undefined}
						onclick={() => onOpen(item)}
					>
						<span class="flex items-baseline gap-2">
							<span
								class="min-w-0 flex-1 truncate text-[13px] {unread
									? 'font-bold text-base-content'
									: 'font-medium text-base-content/80'}"
							>
								{message.from_name || message.from_address || 'Unknown sender'}
							</span>
							<span class="shrink-0 text-2xs text-base-content/50">
								{formatMailDate('sent_at' in message ? message.sent_at : null) ||
									('imported_at' in message ? formatMailDate(message.imported_at) : '')}
							</span>
						</span>
						<span class="mt-0.5 flex items-center gap-1.5">
							<span
								class="min-w-0 flex-1 truncate text-xs {unread
									? 'font-semibold text-base-content'
									: 'text-base-content/60'}"
							>
								{message.subject || '(no subject)'}
							</span>
							{#if 'has_attachments' in message && message.has_attachments}
								<Paperclip size={12} class="shrink-0 text-base-content/45" />
							{/if}
							{#if !isImap && 'source_mode' in message}
								<span
									class="shrink-0 rounded border border-[var(--rs-border)] px-1 text-2xs text-base-content/50"
								>
									{sourceBadge(message.source_mode)}
								</span>
							{/if}
							{#if unread}
								<span class="h-1.5 w-1.5 shrink-0 rounded-full bg-brand-500" aria-label="Unread"
								></span>
							{/if}
						</span>
					</button>
				</div>
			{/each}
			{#if hasMore}
				<div class="flex justify-center py-2">
					<button
						type="button"
						class="btn btn-xs btn-outline"
						disabled={loadingMore}
						onclick={onLoadMore}
					>
						{loadingMore ? 'Loading…' : 'Load more'}
					</button>
				</div>
			{/if}
		{/if}
	</div>
</div>
