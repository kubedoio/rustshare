<script lang="ts">
	import { hasChatKey, exportChatKey, clearChatKey } from '$lib/chat/keys';
	import { lock, chatSessionStore } from '$lib/chat/session';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import { MoreVertical, Lock, Download, Trash2 } from 'lucide-svelte';

	let open = $state(false);
	let confirmRemove = $state(false);
	let notice = $state('');
	let error = $state('');

	const unlocked = $derived($chatSessionStore.state === 'unlocked');
	const keyPresent = $derived(hasChatKey());

	function toggle(): void {
		open = !open;
	}

	function close(): void {
		open = false;
	}

	function handleLock(): void {
		lock();
		close();
	}

	async function handleExport(): Promise<void> {
		try {
			await navigator.clipboard.writeText(exportChatKey());
			notice = 'Backup copied to clipboard.';
			error = '';
		} catch {
			error = 'Could not copy the backup.';
			notice = '';
		}
		close();
	}

	function handleRemove(): void {
		close();
		confirmRemove = true;
	}

	function confirmRemoveKey(): void {
		clearChatKey();
		lock();
		confirmRemove = false;
	}
</script>

<div class="relative">
	<button
		type="button"
		class="btn btn-sm btn-ghost"
		aria-label="Chat identity options"
		aria-expanded={open}
		aria-haspopup="menu"
		onclick={toggle}
	>
		<MoreVertical size={16} />
	</button>

	{#if open}
		<div
			class="absolute bottom-full right-0 z-50 mb-1 w-56 rounded-lg border border-base-300 bg-base-100 py-1 shadow-xl shadow-black/20"
			role="menu"
		>
			{#if unlocked}
				<button
					type="button"
					class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
					role="menuitem"
					onclick={handleLock}
				>
					<Lock size={14} />
					Lock Chat identity
				</button>
			{/if}
			{#if keyPresent}
				<button
					type="button"
					class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-base-200"
					role="menuitem"
					onclick={handleExport}
				>
					<Download size={14} />
					Export key backup
				</button>
				<div class="my-1 border-t border-base-200"></div>
				<button
					type="button"
					class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-error hover:bg-error/10"
					role="menuitem"
					onclick={handleRemove}
				>
					<Trash2 size={14} />
					Remove key from this device
				</button>
			{/if}
		</div>
	{/if}
</div>

{#if notice}<p class="mt-1 text-xs text-success">{notice}</p>{/if}
{#if error}<p class="mt-1 text-xs text-error">{error}</p>{/if}

<ConfirmModal
	open={confirmRemove}
	title="Remove Chat key?"
	message="This removes the encrypted Chat key from this device. You will need your backup to send messages again."
	confirmLabel="Remove"
	cancelLabel="Cancel"
	danger={true}
	onConfirm={confirmRemoveKey}
	onCancel={() => (confirmRemove = false)}
/>
