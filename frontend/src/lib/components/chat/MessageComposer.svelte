<script lang="ts">
	import { untrack } from 'svelte';
	import {
		buildUnsignedEvent,
		publishEvent,
		publishSignedEvent,
		publicKeyOf,
		NOSTR_KIND_STREAM_MESSAGE,
		NOSTR_KIND_TEXT,
		isUuid,
		type NostrTag,
		type NostrEvent
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
		postingAllowed?: boolean;
		observedEventIds?: string[];
		onSendFailure: (message: string) => void;
		onSent?: (eventId: string) => void;
	}

	let {
		relayUrl,
		channelId,
		channelName,
		boundPubkey,
		postingAllowed = false,
		observedEventIds = [],
		onSendFailure,
		onSent = () => {}
	}: Props = $props();

	let draft = $state('');
	let sending = $state(false);
	let pendingEvent = $state<NostrEvent | null>(null);
	let loadedScope = $state('');
	let storageError = $state('');
	let attachmentTag = $state<NostrTag | null>(null);
	let textarea = $state<HTMLTextAreaElement | null>(null);

	const storageKey = $derived(
		`elembra.chat.draft:${JSON.stringify([relayUrl, boundPubkey, channelId])}`
	);

	function saveDraft(key = storageKey): void {
		try {
			if (!draft && !attachmentTag && !pendingEvent) sessionStorage.removeItem(key);
			else sessionStorage.setItem(key, JSON.stringify({ draft, attachmentTag, pendingEvent }));
			storageError = '';
		} catch {
			storageError = 'Browser storage unavailable. Keep this tab open to preserve your draft.';
			throw new Error('Cannot preserve the signed message safely. Browser storage is unavailable.');
		}
	}

	$effect(() => {
		const scope = storageKey;
		untrack(() => {
			draft = '';
			attachmentTag = null;
			pendingEvent = null;
			try {
				const raw = sessionStorage.getItem(scope);
				if (raw) {
					const saved = JSON.parse(raw);
					if (typeof saved.draft === 'string') draft = saved.draft;
					if (
						Array.isArray(saved.attachmentTag) &&
						saved.attachmentTag.every((v: unknown) => typeof v === 'string' || v === null)
					)
						attachmentTag = saved.attachmentTag;
					const event = saved.pendingEvent;
					if (
						event &&
						event.pubkey === boundPubkey &&
						typeof event.id === 'string' &&
						Array.isArray(event.tags) &&
						(isUuid(channelId)
							? event.kind === 9 &&
								event.tags.some((t: string[]) => t[0] === 'h' && t[1] === channelId)
							: event.kind === 1)
					)
						pendingEvent = event;
				}
			} catch {
				storageError = 'Saved draft unavailable. Browser storage may be disabled.';
			}
			loadedScope = scope;
		});
	});

	$effect(() => {
		const scope = storageKey;
		const ready = loadedScope === scope;
		// Track the draft state without letting storage feedback rerun this effect.
		void draft;
		void attachmentTag;
		void pendingEvent;
		if (ready)
			untrack(() => {
				try {
					saveDraft(scope);
				} catch {
					/* Displayed above. */
				}
			});
	});

	$effect(() => {
		if (pendingEvent && observedEventIds.includes(pendingEvent.id)) {
			pendingEvent = null;
			draft = '';
			attachmentTag = null;
			untrack(() => {
				try {
					saveDraft();
				} catch {
					/* Displayed above. */
				}
			});
		}
	});

	function discardPending(): void {
		if (
			!window.confirm(
				'The message may already exist on the relay. Discarding this local copy does not delete it. Continue?'
			)
		)
			return;
		pendingEvent = null;
		draft = '';
		attachmentTag = null;
		try {
			saveDraft();
		} catch {
			/* Displayed above. */
		}
		onSendFailure('');
	}

	const signingKey = $derived(getSigningKey());
	const canSend = $derived(
		postingAllowed &&
			!sending &&
			pendingEvent === null &&
			signingKey !== null &&
			(draft.trim().length > 0 || attachmentTag !== null)
	);

	function adjustHeight(): void {
		if (!textarea) return;
		textarea.style.height = 'auto';
		const maxHeight = 240;
		const nextHeight = Math.min(textarea.scrollHeight, maxHeight);
		textarea.style.height = `${nextHeight}px`;
	}

	async function send(retry = false): Promise<void> {
		if (!postingAllowed || sending || (pendingEvent !== null && !retry)) return;
		const scope = storageKey;
		const author = boundPubkey;
		const endpoint = relayUrl;
		const content = draft.trim();
		if (!content && !attachmentTag && !pendingEvent) return;

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
			if (publicKeyOf(secretKey) !== author) {
				onSendFailure('Local Chat key does not match your bound Buzz identity.');
				return;
			}

			const result =
				retry && pendingEvent
					? await publishSignedEvent(endpoint, pendingEvent, secretKey)
					: await publishEvent(
							endpoint,
							await buildUnsignedEvent(
								streamScoped ? NOSTR_KIND_STREAM_MESSAGE : NOSTR_KIND_TEXT,
								content,
								tags,
								boundPubkey
							),
							secretKey,
							(event) => {
								if (!postingAllowed || storageKey !== scope || getSigningKey() !== secretKey)
									throw new Error('Chat session changed before sending.');
								pendingEvent = event;
								saveDraft(scope);
							}
						);

			if (storageKey !== scope || getSigningKey() !== secretKey) return;

			if (result.ok) {
				pendingEvent = null;
				draft = '';
				attachmentTag = null;
				requestAnimationFrame(adjustHeight);
				onSendFailure('');
				onSent(result.event_id);
			} else if (result.reason === 'uncertain') {
				pendingEvent = result.event;
				saveDraft(scope);
				onSendFailure(
					'Relay acknowledgement missing. The message may have been accepted. Retry only the same signed message.'
				);
			} else if (result.reason === 'rejected') {
				if (!retry) pendingEvent = null;
				const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
				onSendFailure(
					retry
						? `Relay rejected this retry${detail}. The original submission is still uncertain.`
						: `Relay rejected the message${detail}`
				);
			} else {
				// A retry's previous submission is still uncertain even if reconnect fails.
				if (!retry) pendingEvent = null;
				onSendFailure(
					retry
						? 'Relay unreachable. Previous message acceptance is still uncertain.'
						: 'Relay unreachable'
				);
			}
		} catch (err) {
			if (storageKey !== scope || getSigningKey() !== secretKey) return;
			// Defensive: publicKeyOf or buildUnsignedEvent can throw on a malformed
			// key; surface it instead of leaving the user with no feedback.
			onSendFailure(err instanceof Error ? err.message : 'Send failed — try again');
		} finally {
			sending = false;
			if (storageKey === scope) {
				try {
					saveDraft(scope);
				} catch {
					/* Displayed above. */
				}
			}
		}
	}
</script>

<div class="border-t border-base-300 bg-base-100 p-3">
	{#if storageError}<p role="alert" class="mb-2 text-sm text-warning">{storageError}</p>{/if}
	{#if pendingEvent && !sending}
		<div role="status" class="mb-2 text-sm text-warning">
			<p>Message acceptance is uncertain. Editing is paused to prevent duplicate messages.</p>
			<button
				type="button"
				class="btn btn-sm"
				disabled={!postingAllowed || signingKey === null}
				onclick={() => send(true)}>Retry same message</button
			>
			<button type="button" class="btn btn-ghost btn-sm" onclick={discardPending}
				>Discard local copy</button
			>
		</div>
	{/if}
	{#if !postingAllowed}<p role="status" class="mb-2 text-sm text-base-content/60">
			Posting unavailable until your channel permissions are confirmed.
		</p>{/if}
	{#if attachmentTag}
		<div
			class="mb-2 inline-flex items-center gap-2 rounded-lg bg-base-200 px-2 py-1 text-xs text-base-content/80"
		>
			<span class="max-w-[16rem] truncate">Attachment: {attachmentTag[1]}</span>
			<button
				type="button"
				class="text-error hover:underline"
				disabled={sending || pendingEvent !== null}
				onclick={() => (attachmentTag = null)}
			>
				remove
			</button>
		</div>
	{/if}
	<div
		class="flex items-end gap-3 rounded-2xl border border-base-300 bg-base-100 p-2 shadow-sm focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-primary/30"
	>
		<textarea
			bind:this={textarea}
			class="textarea max-h-[240px] min-h-[44px] flex-1 resize-none border-0 textarea-ghost bg-transparent px-2 py-2 text-sm leading-relaxed focus:outline-none"
			rows={1}
			placeholder="Message #{channelName}"
			aria-label="Message text"
			bind:value={draft}
			disabled={sending || pendingEvent !== null}
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
			{#if !sending && pendingEvent === null}<AttachmentPicker
					onSelect={(tag) => (attachmentTag = tag)}
					iconOnly
				/>{/if}
			<button
				type="button"
				class="btn h-8 w-8 rounded-lg btn-ghost p-0 btn-xs"
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
			class="btn inline-flex items-center gap-1.5 rounded-xl px-4 btn-primary btn-sm"
			disabled={!canSend}
			aria-label="Send message"
			onclick={() => send()}
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
