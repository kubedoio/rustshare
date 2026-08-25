<script lang="ts">
	import { onMount } from 'svelte';
	import { hasChatKey, importChatKey } from '$lib/chat/keys';
	import { unlock, ChatSessionError } from '$lib/chat/session';

	interface Props {
		boundPubkey: string;
		onUnlocked: () => void;
	}

	let { boundPubkey, onUnlocked }: Props = $props();

	type Mode = 'locked' | 'missing' | 'corrupt' | 'mismatch';

	let mode = $state<Mode>('locked');
	let passphrase = $state('');
	let unlockError = $state('');
	let unlocking = $state(false);

	let backupJson = $state('');
	let importPassphrase = $state('');
	let importError = $state('');
	let importing = $state(false);

	onMount(() => {
		mode = hasChatKey() ? 'locked' : 'missing';
	});

	async function tryUnlock(): Promise<void> {
		unlockError = '';
		unlocking = true;
		try {
			await unlock(passphrase, boundPubkey);
			passphrase = '';
			onUnlocked();
		} catch (err) {
			if (err instanceof ChatSessionError) {
				if (err.code === 'WRONG_PASSPHRASE') {
					unlockError = err.message;
					return;
				}
				if (err.code === 'CORRUPT_KEY') {
					mode = 'corrupt';
					return;
				}
				if (err.code === 'PUBKEY_MISMATCH') {
					mode = 'mismatch';
					return;
				}
				mode = 'missing';
				return;
			}
			unlockError = err instanceof Error ? err.message : 'Could not unlock Chat identity.';
		} finally {
			unlocking = false;
		}
	}

	async function tryImport(): Promise<void> {
		importError = '';
		if (!backupJson.trim()) {
			importError = 'Paste the key backup first.';
			return;
		}
		importing = true;
		try {
			await importChatKey(backupJson.trim(), importPassphrase);
			// load the just-imported envelope into the in-memory session
			await unlock(importPassphrase, boundPubkey);
			onUnlocked();
		} catch (err) {
			if (err instanceof ChatSessionError && err.code === 'PUBKEY_MISMATCH') {
				importError =
					'That backup is not the identity bound to this account. Paste the backup from your original device, or ask an administrator to rotate the binding.';
			} else {
				importError =
					err instanceof Error ? err.message : 'Import failed — check the backup and passphrase.';
			}
		} finally {
			importing = false;
		}
	}

	const heading = $derived(
		mode === 'locked'
			? 'Unlock Chat'
			: mode === 'missing'
				? 'No Chat identity on this device'
				: mode === 'corrupt'
					? 'Chat key is corrupted'
					: 'Chat key does not match this account'
	);

	const explanation = $derived(
		mode === 'locked'
			? 'Enter your key passphrase to send messages from this device.'
			: mode === 'missing'
				? 'Import the encrypted backup copied from your original device to use your existing identity. Without it, an administrator must rotate your binding.'
				: mode === 'corrupt'
					? 'The stored key cannot be read. Import the backup from your original device to recover your identity.'
					: 'The key stored on this device belongs to a different identity. Import the backup from your original device, or ask an administrator to rotate your binding.'
	);
</script>

<div class="border-t border-base-300 p-4">
	<h3 class="mb-1 text-sm font-semibold">{heading}</h3>
	<p class="mb-3 text-sm text-base-content/60">{explanation}</p>

	{#if mode === 'locked'}
		<div class="flex gap-2">
			<input
				type="password"
				class="input input-sm flex-1"
				placeholder="key passphrase"
				aria-label="key passphrase"
				bind:value={passphrase}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						tryUnlock();
					}
				}}
			/>
			<button type="button" class="btn btn-sm btn-primary" disabled={unlocking} onclick={tryUnlock}>
				{unlocking ? 'Unlocking…' : 'Unlock'}
			</button>
		</div>
		{#if unlockError}<p class="mt-2 text-sm text-error">{unlockError}</p>{/if}
	{:else}
		<label class="mb-1 block text-xs" for="chat-key-backup">Key backup</label>
		<textarea
			id="chat-key-backup"
			rows={2}
			class="textarea textarea-sm mb-2 w-full font-mono text-xs"
			placeholder={`Paste the "Export key backup" contents`}
			bind:value={backupJson}></textarea>
		<div class="flex gap-2">
			<input
				type="password"
				class="input input-sm flex-1"
				placeholder="backup passphrase"
				aria-label="backup passphrase"
				bind:value={importPassphrase}
			/>
			<button type="button" class="btn btn-sm btn-primary" disabled={importing} onclick={tryImport}>
				{importing ? 'Importing…' : 'Import key'}
			</button>
		</div>
		{#if importError}<p class="mt-2 text-sm text-error">{importError}</p>{/if}
	{/if}
</div>
