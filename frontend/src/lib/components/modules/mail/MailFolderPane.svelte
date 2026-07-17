<script lang="ts">
	import {
		Inbox,
		Send,
		FileEdit,
		Archive,
		Trash2,
		Folder,
		AlertTriangle,
		RefreshCw,
		HardDriveDownload
	} from 'lucide-svelte';
	import type { MailFolder } from '$lib/api/mail';
	import type { FolderSelection } from './mail-types';

	let {
		folders,
		foldersLoading,
		foldersError,
		draftsCount,
		selection,
		onSelect,
		onRetryFolders
	}: {
		folders: MailFolder[];
		foldersLoading: boolean;
		foldersError: string | null;
		draftsCount: number | null;
		selection: FolderSelection;
		onSelect: (selection: FolderSelection) => void;
		onRetryFolders: () => void;
	} = $props();

	function folderIcon(folder: MailFolder) {
		const name = folder.display_name.toLowerCase();
		if (folder.role === 'sent' || name === 'sent' || name.includes('sent')) return Send;
		if (folder.role === 'drafts' || name.includes('draft')) return FileEdit;
		if (folder.role === 'archive' || name.includes('archive') || name.includes('all mail'))
			return Archive;
		if (folder.role === 'trash' || name.includes('trash') || name.includes('deleted'))
			return Trash2;
		if (name === 'inbox') return Inbox;
		return Folder;
	}

	function isSelected(kind: FolderSelection['kind'], name?: string): boolean {
		if (selection.kind !== kind) return false;
		if (kind === 'imap') return selection.kind === 'imap' && selection.name === name;
		return true;
	}

	const itemBase =
		'flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors';
	const itemIdle = 'text-base-content/75 hover:bg-base-200';
	const itemActive = 'bg-base-200 font-medium text-base-content';
</script>

<nav aria-label="Mail folders" class="flex h-full min-h-0 flex-col">
	<div class="flex-1 overflow-y-auto p-2">
		<p class="px-2.5 pb-1 pt-1 text-2xs font-semibold uppercase tracking-wide text-base-content/45">
			RustShare
		</p>
		<div class="flex flex-col gap-0.5" role="listbox" aria-label="Local folders">
			<button
				type="button"
				class="{itemBase} {isSelected('imported') ? itemActive : itemIdle}"
				role="option"
				aria-selected={isSelected('imported')}
				aria-current={isSelected('imported') ? 'true' : undefined}
				onclick={() => onSelect({ kind: 'imported' })}
			>
				<HardDriveDownload size={15} class="shrink-0 text-base-content/50" />
				<span class="flex-1 truncate">Imported</span>
			</button>
			<button
				type="button"
				class="{itemBase} {isSelected('drafts') ? itemActive : itemIdle}"
				role="option"
				aria-selected={isSelected('drafts')}
				aria-current={isSelected('drafts') ? 'true' : undefined}
				onclick={() => onSelect({ kind: 'drafts' })}
			>
				<FileEdit size={15} class="shrink-0 text-base-content/50" />
				<span class="flex-1 truncate">Drafts</span>
				{#if draftsCount}
					<span class="text-xs text-base-content/50">{draftsCount}</span>
				{/if}
			</button>
		</div>

		<p class="px-2.5 pb-1 pt-4 text-2xs font-semibold uppercase tracking-wide text-base-content/45">
			IMAP folders
		</p>
		{#if foldersLoading}
			<div class="flex flex-col gap-1.5 px-2 py-1" aria-label="Loading folders">
				{#each [0, 1, 2, 3] as i}
					<div class="skeleton h-6 w-full" class:opacity-60={i > 1}></div>
				{/each}
			</div>
		{:else if foldersError}
			<div class="mx-1 rounded-md border border-warning/40 bg-warning/5 p-2.5" role="alert">
				<div class="flex items-start gap-2">
					<AlertTriangle size={15} class="mt-0.5 shrink-0 text-warning" />
					<div class="min-w-0 flex-1">
						<p class="text-xs font-semibold text-base-content">Folders could not be refreshed.</p>
						<p class="mt-0.5 truncate text-2xs text-base-content/55" title={foldersError}>
							{foldersError}
						</p>
						<p class="mt-1 text-2xs text-base-content/55">
							Imported mail and drafts remain available.
						</p>
						<button
							type="button"
							class="btn btn-xs btn-outline mt-2 gap-1"
							onclick={onRetryFolders}
						>
							<RefreshCw size={11} /> Retry
						</button>
					</div>
				</div>
			</div>
		{:else if folders.length === 0}
			<p class="px-2.5 py-1 text-xs text-base-content/50">No remote folders returned.</p>
		{:else}
			<div class="flex flex-col gap-0.5" role="listbox" aria-label="IMAP folders">
				{#each folders as folder}
					{@const Icon = folderIcon(folder)}
					<button
						type="button"
						class="{itemBase} {isSelected('imap', folder.name) ? itemActive : itemIdle}"
						role="option"
						aria-selected={isSelected('imap', folder.name)}
						aria-current={isSelected('imap', folder.name) ? 'true' : undefined}
						title={folder.display_name}
						onclick={() => onSelect({ kind: 'imap', name: folder.name })}
					>
						<Icon size={15} class="shrink-0 text-base-content/50" />
						<span class="flex-1 truncate">{folder.display_name}</span>
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="border-t border-[var(--rs-border)] p-2">
		<a
			href="/settings?tab=mail"
			class="flex items-center gap-2 rounded-md px-2.5 py-1.5 text-xs text-base-content/60 hover:bg-base-200 hover:text-base-content"
		>
			Manage accounts
		</a>
	</div>
</nav>
