<script lang="ts">
	import {
		buildUnsignedEvent,
		publishEvent,
		publicKeyOf,
		NOSTR_KIND_STREAM_MESSAGE,
		NOSTR_KIND_TEXT,
		isUuid,
		type NostrTag
	} from '$lib/chat/nostr';
	import { getSigningKey } from '$lib/chat/session';
	import AttachmentPicker from './AttachmentPicker.svelte';
	import ChatIdentityMenu from './ChatIdentityMenu.svelte';

	interface Props {
		relayUrl: string;
		channelId: string;
		// The bound Buzz pubkey comes from the parent's Chat status. The composer
		// assumes the Chat identity session is already unlocked and only verifies
		// that the in-memory key matches this pubkey before signing.
		boundPubkey: string;
		onSendFailure: (message: string) => void;
		onSent?: (eventId: string) => void;
	}

	let { relayUrl, channelId, boundPubkey, onSendFailure, onSent = () => {} }: Props = $props();

	let draft = $state('');
	let sending = $state(false);
	let attachmentTag = $state<NostrTag | null>(null);

	const signingKey = $derived(getSigningKey());
	const canSend = $derived(
		!sending && signingKey !== null && (draft.trim().length > 0 || attachmentTag !== null)
	);

	async function send(): Promise<void> {
		if (sending) return;
		const content = draft.trim();
		if (!content && !attachmentTag) return;

		const secretKey = signingKey;
		if (!secretKey) {
			onSendFailure('Chat identity is locked.');
			return;
		}

		// Canonical chat wire format (spec: "Canonical publish tags and kinds"):
		// kind-9 stream messages are channel-scoped by the NIP-29 `["h",
		// "<channel-uuid>"]` tag. The relay parses `h` strictly as a channel UUID,
		// so the stream publish is gated on the active channel id being a canonical
		// UUID. Name-based channels (observation-derived ids like 'general') fall
		// back to a legacy kind-1 note with no h tag, served by the observation
		// path. Thread/reply e-tags are a later feature (issue #243), so no thread
		// tags are emitted here.
		const streamScoped = isUuid(channelId);
		const tags: NostrTag[] = [];
		if (streamScoped) tags.push(['h', channelId]);
		if (attachmentTag) tags.push(attachmentTag);

		sending = true;
		try {
			if (publicKeyOf(secretKey) !== boundPubkey) {
				onSendFailure('Local Chat key does not match your bound Buzz identity.');
				return;
			}

			const result = await publishEvent(
				relayUrl,
				await buildUnsignedEvent(
					streamScoped ? NOSTR_KIND_STREAM_MESSAGE : NOSTR_KIND_TEXT,
					content,
					tags,
					boundPubkey
				),
				secretKey
			);

			if (result.ok) {
				draft = '';
				attachmentTag = null;
				onSendFailure('');
				onSent(result.event_id);
			} else if (result.reason === 'rejected') {
				const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
				onSendFailure(`Relay rejected the message${detail}`);
			} else {
				onSendFailure('Relay unreachable');
			}
		} catch (err) {
			// Defensive: publicKeyOf or buildUnsignedEvent can throw on a malformed
			// key; surface it instead of leaving the user with no feedback.
			onSendFailure(err instanceof Error ? err.message : 'Send failed — try again');
		} finally {
			sending = false;
		}
	}
</script>

<div class="border-t border-base-300 p-3">
	{#if attachmentTag}
		<div class="mb-2 text-xs text-base-content/60">
			Attachment: {attachmentTag[1]}
			<button type="button" class="ml-2 text-error" onclick={() => (attachmentTag = null)}>
				remove
			</button>
		</div>
	{/if}
	<div class="flex items-end gap-2">
		<AttachmentPicker onSelect={(tag) => (attachmentTag = tag)} />
		<textarea
			class="textarea textarea-sm min-h-0 flex-1"
			rows={2}
			placeholder="Message #{channelId}"
			aria-label="Message text"
			bind:value={draft}
			onkeydown={(e) => {
				if (e.key === 'Enter' && !e.shiftKey) {
					e.preventDefault();
					if (canSend) send();
				}
			}}></textarea>
		<button
			type="button"
			class="btn btn-sm btn-primary"
			disabled={!canSend}
			aria-label="Send message"
			onclick={send}
		>
			{sending ? 'Sending…' : 'Send'}
		</button>
		<ChatIdentityMenu />
	</div>
</div>
