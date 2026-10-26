<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
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
	let active = true;
	onDestroy(() => {
		active = false;
	});
	function bindingGuard(pubkey: string): void {
		if (!active || boundPubkey !== pubkey) throw new Error('Chat binding changed. Try again.');
	}

	onMount(() => {
		mode = hasChatKey() ? 'locked' : 'missing';
	});

	async function tryUnlock(): Promise<void> {
		if (unlocking || importing) return;
		unlockError = '';
		unlocking = true;
		const pubkey = boundPubkey;
		try {
			await unlock(passphrase, pubkey, () => bindingGuard(pubkey));
			passphrase = '';
			onUnlocked();
		} catch (err) {
			if (err instanceof ChatSessionError) {
				if (err.code === 'WRONG_PASSPHRASE' || err.code === 'RETRY') {
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
				if (err.code === 'SESSION_CHANGED') {
					unlockError = err.message;
					return;
				}
				mode = 'missing';
				return;
			}
			unlockError = err instanceof Error ? err.message : 'Could not unlock Chat identity.';
		} finally {
			passphrase = '';
			unlocking = false;
		}
	}

	async function tryImport(): Promise<void> {
		if (unlocking || importing) return;
		importError = '';
		if (!backupJson.trim()) {
			importError = 'Paste the key backup first.';
			importPassphrase = '';
			backupJson = '';
			return;
		}
		importing = true;
		const pubkey = boundPubkey;
		try {
			await importChatKey(backupJson.trim(), importPassphrase, pubkey, false, () =>
				bindingGuard(pubkey)
			);
			// load the just-imported envelope into the in-memory session
			await unlock(importPassphrase, pubkey, () => bindingGuard(pubkey));
			onUnlocked();
		} catch (err) {
			importError =
				err instanceof Error ? err.message : 'Import failed — check the backup and passphrase.';
		} finally {
			importPassphrase = '';
			backupJson = '';
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
				class="input flex-1 input-sm"
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
			<button type="button" class="btn btn-primary btn-sm" disabled={unlocking} onclick={tryUnlock}>
				{unlocking ? 'Unlocking…' : 'Unlock'}
			</button>
		</div>
		{#if unlockError}<p role="alert" class="mt-2 text-sm text-error">{unlockError}</p>{/if}
		<button
			type="button"
			class="btn btn-ghost btn-sm"
			disabled={unlocking}
			onclick={() => {
				passphrase = '';
				mode = 'missing';
			}}>Recover from backup</button
		>
	{:else}
		<label class="mb-1 block text-xs" for="chat-key-backup">Key backup</label>
		<textarea
			id="chat-key-backup"
			rows={2}
			maxlength={4096}
			class="textarea mb-2 w-full font-mono text-xs textarea-sm"
			placeholder={`Paste the "Export key backup" contents`}
			bind:value={backupJson}></textarea>
		<div class="flex gap-2">
			<input
				type="password"
				class="input flex-1 input-sm"
				placeholder="backup passphrase"
				aria-label="backup passphrase"
				bind:value={importPassphrase}
			/>
			<button type="button" class="btn btn-primary btn-sm" disabled={importing} onclick={tryImport}>
				{importing ? 'Importing…' : 'Import key'}
			</button>
		</div>
		{#if importError}<p role="alert" class="mt-2 text-sm text-error">{importError}</p>{/if}
	{/if}
</div>
