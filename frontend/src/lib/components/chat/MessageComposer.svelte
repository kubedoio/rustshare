<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getChatStatus } from '$lib/api/chat';
	import {
		buildUnsignedEvent,
		publishEvent,
		publicKeyOf,
		NOSTR_KIND_TEXT,
		type NostrTag
	} from '$lib/chat/nostr';
	import { hasChatKey, loadChatKey } from '$lib/chat/keys';
	import AttachmentPicker from './AttachmentPicker.svelte';

	interface Props {
		relayUrl: string;
		channelId: string;
		onSendFailure: (message: string) => void;
	}

	let { relayUrl, channelId, onSendFailure }: Props = $props();

	const statusQuery = createQuery({
		queryKey: ['chat-status'],
		queryFn: () => getChatStatus(),
		staleTime: 30_000
	});

	let draft = $state('');
	let sending = $state(false);
	let passphrase = $state('');
	let needsPassphrase = $state(false);
	let attachmentTag = $state<NostrTag | null>(null);

	async function send(): Promise<void> {
		if (sending) return; // guard against double-publish via Enter during an in-flight send
		const content = draft.trim();
		if (!content && !attachmentTag) return;
		const status = $statusQuery.data;
		if (!status?.binding) return;
		const secretKey = await unlockKey();
		if (!secretKey) return;
		if (publicKeyOf(secretKey) !== status.binding.buzz_pubkey) {
			onSendFailure('local key does not match your bound Buzz identity');
			return;
		}
		// No channel tag is added here: channel attribution is determined by
		// the Buzz bridge under the current contract, and a client channel-tag
		// wire format is deferred upstream until confirmed (spec §10, same
		// status as thread tags).
		const tags: NostrTag[] = [];
		if (attachmentTag) tags.push(attachmentTag);
		sending = true;
		const ok = await publishEvent(
			relayUrl,
			await buildUnsignedEvent(NOSTR_KIND_TEXT, content, tags, status.binding.buzz_pubkey),
			secretKey
		);
		sending = false;
		if (ok) {
			draft = '';
			attachmentTag = null;
			onSendFailure('');
		} else {
			onSendFailure('relay unreachable');
		}
	}

	async function unlockKey(): Promise<string | null> {
		if (!hasChatKey()) {
			onSendFailure('no local chat key — bind your identity first');
			return null;
		}
		try {
			return await loadChatKey(passphrase || '');
		} catch {
			needsPassphrase = true;
			return null;
		}
	}
</script>

<div class="border-t border-base-300 p-3">
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
