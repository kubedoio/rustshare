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
	import { Send, Smile } from 'lucide-svelte';

	interface Props {
		relayUrl: string;
		channelId: string;
		channelName: string;
		// The bound Buzz pubkey comes from the parent's Chat status. The composer
		// assumes the Chat identity session is already unlocked and only verifies
		// that the in-memory key matches this pubkey before signing.
		boundPubkey: string;
		onSendFailure: (message: string) => void;
		onSent?: (eventId: string) => void;
	}

	let {
		relayUrl,
		channelId,
		channelName,
		boundPubkey,
		onSendFailure,
		onSent = () => {}
	}: Props = $props();

	let draft = $state('');
	let sending = $state(false);
	let attachmentTag = $state<NostrTag | null>(null);
	let textarea = $state<HTMLTextAreaElement | null>(null);

	const signingKey = $derived(getSigningKey());
	const canSend = $derived(
		!sending && signingKey !== null && (draft.trim().length > 0 || attachmentTag !== null)
	);

	function adjustHeight(): void {
		if (!textarea) return;
		textarea.style.height = 'auto';
		const maxHeight = 240;
		const nextHeight = Math.min(textarea.scrollHeight, maxHeight);
		textarea.style.height = `${nextHeight}px`;
	}

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
				requestAnimationFrame(adjustHeight);
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

<div class="border-t border-base-300 bg-base-100 p-3">
	{#if attachmentTag}
		<div
			class="mb-2 inline-flex items-center gap-2 rounded-lg bg-base-200 px-2 py-1 text-xs text-base-content/80"
		>
			<span class="truncate max-w-[16rem]">Attachment: {attachmentTag[1]}</span>
			<button
				type="button"
				class="text-error hover:underline"
				onclick={() => (attachmentTag = null)}
			>
				remove
			</button>
		</div>
	{/if}
	<div
		class="flex items-end gap-3 rounded-2xl border border-base-300 bg-base-100 p-2 shadow-sm focus-within:ring-2 focus-within:ring-primary/30 focus-within:border-primary/50"
	>
		<textarea
			bind:this={textarea}
			class="textarea textarea-ghost min-h-[44px] max-h-[240px] flex-1 resize-none border-0 bg-transparent px-2 py-2 text-sm leading-relaxed focus:outline-none"
			rows={1}
			placeholder="Message #{channelName}"
			aria-label="Message text"
			bind:value={draft}
			oninput={adjustHeight}
			onkeydown={(e) => {
				if (e.key === 'Enter' && !e.shiftKey) {
					e.preventDefault();
					if (canSend) send();
				}
			}}></textarea>
	</div>

	<div class="mt-2 flex items-center justify-between px-1">
		<div class="flex items-center gap-1">
			<AttachmentPicker onSelect={(tag) => (attachmentTag = tag)} iconOnly />
			<button
				type="button"
				class="btn btn-ghost btn-xs h-8 w-8 rounded-lg p-0"
				aria-label="Add emoji"
				title="Emoji (coming soon)"
				disabled
			>
				<Smile size={16} class="text-base-content/60" />
			</button>
			<ChatIdentityMenu />
		</div>

		<button
			type="button"
			class="btn btn-sm btn-primary inline-flex items-center gap-1.5 rounded-xl px-4"
			disabled={!canSend}
			aria-label="Send message"
			onclick={send}
		>
			{#if sending}
				<span class="loading loading-xs loading-spinner"></span>
				Sending…
			{:else}
				<Send size={16} />
			{/if}
		</button>
	</div>
</div>
