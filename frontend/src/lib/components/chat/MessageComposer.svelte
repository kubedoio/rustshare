<script lang="ts">
	import {
		buildUnsignedEvent,
		publishEvent,
		publicKeyOf,
		NOSTR_KIND_TEXT,
		type NostrTag
	} from '$lib/chat/nostr';
	import { hasChatKey, loadChatKey, importChatKey, exportChatKey } from '$lib/chat/keys';
	import AttachmentPicker from './AttachmentPicker.svelte';

	interface Props {
		relayUrl: string;
		channelId: string;
		// The bound Buzz pubkey comes from the parent's Chat status (already
		// loaded and reactive there), so the composer does not re-fetch status
		// and never races an unloaded query on the first send.
		boundPubkey: string | null;
		onSendFailure: (message: string) => void;
	}

	let { relayUrl, channelId, boundPubkey, onSendFailure }: Props = $props();

	let draft = $state('');
	let sending = $state(false);
	let passphrase = $state('');
	let needsPassphrase = $state(false);
	let attachmentTag = $state<NostrTag | null>(null);

	// A bound identity without a local key is the multi-device dead-end: the
	// backend binding/admission are server state, so a second browser reaches
	// the composer but holds no key. Importing the encrypted backup is the only
	// recovery path (ADR-0034: no silent server custody).
	let keyMissing = $state(!hasChatKey());
	let backupJson = $state('');
	let importPassphrase = $state('');
	let importing = $state(false);
	let importNotice = $state('');
	let importError = $state('');

	async function importKey(): Promise<void> {
		if (!backupJson.trim()) {
			importError = 'paste the backup copied from your other device first';
			return;
		}
		importing = true;
		importError = '';
		try {
			await importChatKey(backupJson.trim(), importPassphrase);
			keyMissing = false;
			// The passphrase that just decrypted the backup is the key
			// passphrase — seed it so the first send does not re-prompt.
			passphrase = importPassphrase;
			backupJson = '';
			importPassphrase = '';
			importNotice = 'Key imported — you can send as your bound identity.';
			onSendFailure('');
		} catch (err) {
			importError =
				err instanceof Error ? err.message : 'import failed — check the backup and passphrase';
		} finally {
			importing = false;
		}
	}

	function exportBackup(): void {
		try {
			navigator.clipboard.writeText(exportChatKey());
			importNotice = 'Backup copied to clipboard.';
			importError = '';
		} catch {
			importError = 'could not copy the backup';
		}
	}

	async function send(): Promise<void> {
		if (sending) return; // guard against double-publish via Enter during an in-flight send
		const content = draft.trim();
		if (!content && !attachmentTag) return;
		if (keyMissing) {
			onSendFailure('no chat key on this device — import your backup to send');
			return;
		}
		if (!boundPubkey) return;
		// Latch BEFORE the first await: unlockKey runs the full PBKDF2 (hundreds
		// of ms) and two rapid Enters must not both reach publishEvent.
		sending = true;
		try {
			const secretKey = await unlockKey();
			if (!secretKey) return;
			if (publicKeyOf(secretKey) !== boundPubkey) {
				onSendFailure('local key does not match your bound Buzz identity');
				return;
			}
			// No channel tag is added here: channel attribution is determined by
			// the Buzz bridge under the current contract, and a client channel-tag
			// wire format is deferred upstream until confirmed (spec §10, same
			// status as thread tags).
			const tags: NostrTag[] = [];
			if (attachmentTag) tags.push(attachmentTag);
			const result = await publishEvent(
				relayUrl,
				await buildUnsignedEvent(NOSTR_KIND_TEXT, content, tags, boundPubkey),
				secretKey
			);
			if (result.ok) {
				draft = '';
				attachmentTag = null;
				onSendFailure('');
			} else if (result.reason === 'rejected') {
				const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
				onSendFailure(`relay rejected the message${detail}`);
			} else {
				onSendFailure('relay unreachable');
			}
		} finally {
			sending = false;
		}
	}

	async function unlockKey(): Promise<string | null> {
		if (!hasChatKey()) {
			onSendFailure('no chat key on this device — import your backup to send');
			return null;
		}
		try {
			return await loadChatKey(passphrase || '');
		} catch (err) {
			const message = err instanceof Error ? err.message : '';
			if (message === 'no stored chat key') {
				onSendFailure('no local chat key — bind your identity first');
				return null;
			}
			if (message === 'unsupported chat key format') {
				onSendFailure('stored chat key format is unsupported — re-import your backup');
				return null;
			}
			// Decrypt failure: most likely a wrong passphrase.
			needsPassphrase = true;
			return null;
		}
	}
</script>

<div class="border-t border-base-300 p-3">
	{#if keyMissing}
		<div class="mb-2 text-sm">
			<p class="mb-1 text-base-content/60">
				No chat key on this device. Import the backup copied from your original device to keep your
				identity — without it, an administrator must rotate your binding.
			</p>
			<label class="mb-1 block text-xs" for="chat-key-backup">Key backup</label>
			<textarea
				id="chat-key-backup"
				rows={2}
				class="textarea textarea-sm mb-1 w-full font-mono text-xs"
				placeholder={'Paste the "Export key backup" contents'}
				bind:value={backupJson}></textarea>
			<div class="flex gap-2">
				<input
					id="chat-key-passphrase"
					type="password"
					class="input input-sm flex-1"
					placeholder="backup passphrase"
					aria-label="backup passphrase"
					bind:value={importPassphrase}
				/>
				<button
					type="button"
					class="btn btn-sm btn-primary"
					disabled={importing}
					onclick={importKey}
				>
					{importing ? 'Importing…' : 'Import key'}
				</button>
			</div>
			{#if importError}<p class="mt-1 text-sm text-error">{importError}</p>{/if}
			{#if importNotice}<p class="mt-1 text-sm text-success">{importNotice}</p>{/if}
		</div>
	{/if}
	{#if attachmentTag}
		<div class="mb-1 text-xs text-base-content/60">
			Attachment: {attachmentTag[1]}
			<button type="button" class="ml-2 text-error" onclick={() => (attachmentTag = null)}>
				remove
			</button>
		</div>
	{/if}
	{#if needsPassphrase}
		<div class="mb-1 flex gap-2">
			<input
				type="password"
				class="input input-sm"
				placeholder="key passphrase"
				bind:value={passphrase}
			/>
			<button
				type="button"
				class="btn btn-sm"
				onclick={async () => {
					needsPassphrase = false;
					await send();
				}}
			>
				unlock
			</button>
		</div>
	{/if}
	<div class="flex items-end gap-2">
		{#if !keyMissing}
			<button
				type="button"
				class="btn btn-sm"
				title="Copy the encrypted key backup for another device"
				aria-label="Export key backup"
				onclick={exportBackup}
			>
				Export key
			</button>
		{/if}
		<AttachmentPicker onSelect={(tag) => (attachmentTag = tag)} />
		<textarea
			class="textarea textarea-sm min-h-0 flex-1"
			rows={2}
			placeholder="Message #{channelId}"
			bind:value={draft}
			onkeydown={(e) => {
				if (e.key === 'Enter' && !e.shiftKey) {
					e.preventDefault();
					send();
				}
			}}></textarea>
		<button type="button" class="btn btn-sm btn-primary" disabled={sending} onclick={send}>
			{sending ? 'Sending…' : 'Send'}
		</button>
	</div>
</div>
