<script lang="ts">
	import { getSigningKey } from '$lib/chat/session';
	import {
		createChannel,
		retryChannelMutation,
		type ChannelMutationInput,
		type ChannelMutationResult
	} from '$lib/chat/channels';
	import type { NostrEvent } from '$lib/chat/nostr';

	interface Props {
		relayUrl: string;
		relayPubkey: string;
		onCreated: (channelId: string) => void;
	}
	let { relayUrl, relayPubkey, onCreated }: Props = $props();

	let open = $state(false);
	let name = $state('');
	let description = $state('');
	let visibility = $state<'open' | 'private'>('open');
	let pending = $state(false);
	let formError = $state('');
	// An uncertain create keeps the exact signed event and its relay-state
	// confirmation, so a retry replays identical bytes — never a re-sign.
	let pendingRetry = $state<{
		event: NostrEvent;
		confirm: () => Promise<boolean>;
	} | null>(null);

	function mutationInput(): ChannelMutationInput | null {
		const secretKey = getSigningKey();
		if (!secretKey) {
			formError = 'Chat identity is locked.';
			return null;
		}
		return { relayUrl, secretKey, relayPubkey };
	}

	function present(result: ChannelMutationResult): void {
		if (result.status === 'confirmed') {
			const created = createdChannelId;
			pendingRetry = null;
			open = false;
			name = '';
			description = '';
			visibility = 'open';
			if (created) onCreated(created);
		} else if (result.status === 'unconfirmed') {
			formError =
				'The relay accepted the request but the channel is not visible yet. It may still appear in the directory once connected.';
		} else if (result.status === 'uncertain') {
			formError =
				'Relay acknowledgement missing. The channel may already exist — retry only the same signed request.';
		} else if (result.status === 'rejected') {
			const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
			formError = `Relay rejected the channel${detail}`;
		} else {
			formError = 'Relay unreachable. Retry when connected.';
		}
	}

	let createdChannelId = $state<string | null>(null);

	async function submit(): Promise<void> {
		const trimmed = name.trim();
		if (!trimmed) {
			formError = 'Channel name is required.';
			return;
		}
		if (trimmed.startsWith('#')) {
			formError = 'Channel names must not start with #.';
			return;
		}
		const input = mutationInput();
		if (!input) return;
		pending = true;
		formError = '';
		try {
			let confirm: (() => Promise<boolean>) | null = null;
			const { channelId, result } = await createChannel(
				{ ...input, onConfirm: (c) => (confirm = c) },
				{ name: trimmed, description: description.trim(), visibility }
			);
			if (!getSigningKey()) return;
			createdChannelId = channelId;
			if (result.status === 'uncertain') {
				pendingRetry = {
					event: result.event,
					confirm: confirm ?? (() => Promise.resolve(true))
				};
			}
			present(result);
		} finally {
			pending = false;
		}
	}

	async function retry(): Promise<void> {
		const input = mutationInput();
		if (!input || !pendingRetry || pending) return;
		pending = true;
		formError = '';
		try {
			const current = pendingRetry;
			const result = await retryChannelMutation(input, current.event, current.confirm);
			if (!getSigningKey()) return;
			present(result);
			if (result.status !== 'uncertain') pendingRetry = null;
		} finally {
			pending = false;
		}
	}
</script>

<div class="border-b border-base-300 px-3 py-2">
	{#if !open}
		<button
			type="button"
			class="btn w-full btn-ghost btn-xs"
			onclick={() => {
				open = true;
				formError = '';
			}}>+ New channel</button
		>
	{:else}
		<form
			class="space-y-2"
			aria-label="Create channel"
			onsubmit={(e) => {
				e.preventDefault();
				void submit();
			}}
		>
			<div>
				<label class="label text-xs" for="channel-create-name">Name</label>
				<input
					id="channel-create-name"
					class="input-bordered input w-full input-sm"
					aria-label="New channel name"
					bind:value={name}
					disabled={pending || pendingRetry !== null}
				/>
			</div>
			<div>
				<label class="label text-xs" for="channel-create-description">Description</label>
				<textarea
					id="channel-create-description"
					class="textarea-bordered textarea w-full"
					aria-label="New channel description"
					rows={2}
					bind:value={description}
					disabled={pending || pendingRetry !== null}></textarea>
			</div>
			<div>
				<label class="label text-xs" for="channel-create-visibility">Visibility</label>
				<select
					id="channel-create-visibility"
					class="select-bordered select w-full select-sm"
					aria-label="New channel visibility"
					value={visibility}
					onchange={(event) =>
						(visibility = event.currentTarget.value === 'private' ? 'private' : 'open')}
					disabled={pending || pendingRetry !== null}
				>
					<option value="open">Open — anyone in the workspace can join</option>
					<option value="private">Private — only listed members can see it</option>
				</select>
			</div>
			<div class="flex gap-2">
				<button
					type="submit"
					class="btn btn-primary btn-xs"
					disabled={pending || pendingRetry !== null}>Create channel</button
				>
				<button
					type="button"
					class="btn btn-ghost btn-xs"
					disabled={pending}
					onclick={() => {
						open = false;
						formError = '';
						pendingRetry = null;
					}}>Cancel</button
				>
			</div>
		</form>
	{/if}
	{#if pending}<p role="status">Creating channel…</p>{/if}
	{#if formError}<p role="alert" class="text-error">{formError}</p>{/if}
	{#if pendingRetry && !pending}
		<div role="status" class="text-warning">
			<p>Creation is uncertain. Retry only the same signed request.</p>
			<button type="button" class="btn btn-sm" onclick={() => void retry()}
				>Retry same request</button
			>
		</div>
	{/if}
</div>
