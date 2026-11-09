<script lang="ts">
	import { onMount } from 'svelte';
	import { queryRelayEvents, type NostrEvent } from '$lib/chat/nostr';
	import { getSigningKey } from '$lib/chat/session';
	import {
		parseChannelState,
		canPostToChannel,
		type ChatChannelState
	} from '$lib/chat/capabilities';
	import {
		addChannelMember,
		canAddMembers,
		canEditChannelMetadata,
		canJoinChannel,
		canRemoveMembers,
		editChannel,
		isLastOwner,
		joinChannel,
		leaveChannel,
		removeChannelMember,
		retryChannelMutation,
		type ChannelMutationInput,
		type ChannelMutationResult
	} from '$lib/chat/channels';

	interface Props {
		relayUrl: string;
		relayPubkey: string;
		channelId: string;
		pubkey: string;
		onPostingChange: (allowed: boolean) => void;
		// Called after a confirmed mutation so the parent can refresh the
		// relay-backed directory (membership changes what is listed).
		onChanged?: () => void;
	}
	let { relayUrl, relayPubkey, channelId, pubkey, onPostingChange, onChanged }: Props = $props();
	let channelState = $state<ChatChannelState | null>(null);
	let loading = $state(true);
	let error = $state('');
	let detailsOpen = $state(false);
	let generation = 0;

	// Channel management state. Mutations are signed with the in-memory Chat
	// key and confirmed against fresh relay-signed snapshots; an uncertain
	// outcome keeps the exact signed event for a byte-identical retry.
	let opPending = $state(false);
	let opError = $state('');
	let opNotice = $state('');
	let pendingRetry = $state<{
		event: NostrEvent;
		confirm: () => Promise<boolean>;
		success: string;
	} | null>(null);
	let editing = $state(false);
	let editName = $state('');
	let editDescription = $state('');
	let addPubkey = $state('');

	async function refresh(): Promise<void> {
		const current = ++generation;
		onPostingChange(false);
		const key = getSigningKey();
		if (!key) return;
		loading = true;
		error = '';
		const result = await queryRelayEvents(
			relayUrl,
			{ kinds: [39000, 39002], '#d': [channelId] },
			key,
			relayPubkey
		);
		if (current !== generation || key !== getSigningKey()) return;
		loading = false;
		if (!result.ok) {
			channelState = null;
			error = 'Channel permissions unavailable. Posting is disabled. Retry when connected.';
			return;
		}
		channelState = parseChannelState(result.events, channelId, pubkey);
		if (!channelState)
			error = 'Channel metadata or membership could not be confirmed. Posting is disabled.';
		onPostingChange(canPostToChannel(channelState));
	}

	function mutationInput() {
		const secretKey = getSigningKey();
		if (!secretKey) {
			opError = 'Chat identity is locked.';
			return null;
		}
		return { relayUrl, secretKey, relayPubkey };
	}

	function applyResult(result: ChannelMutationResult, success: string): void {
		opNotice = '';
		if (result.status === 'confirmed') {
			pendingRetry = null;
			opError = '';
			opNotice = success;
			void refresh();
			onChanged?.();
		} else if (result.status === 'unconfirmed') {
			opError =
				'The relay accepted the request but the change is not visible yet. It may still apply; refresh to re-check.';
		} else if (result.status === 'uncertain') {
			opError =
				'Relay acknowledgement missing. The operation may already have been accepted — retry only the same signed request.';
		} else if (result.status === 'rejected') {
			const detail = result.detail ? `: ${result.detail.slice(0, 200)}` : '';
			opError = `Relay rejected the operation${detail}`;
		} else {
			opError = 'Relay unreachable. Retry when connected.';
		}
	}

	async function run(
		operation: (input: ChannelMutationInput) => Promise<ChannelMutationResult>,
		success: string
	): Promise<ChannelMutationResult | null> {
		if (opPending) return null;
		const input = mutationInput();
		if (!input) return null;
		opPending = true;
		opError = '';
		opNotice = '';
		try {
			let confirm: (() => Promise<boolean>) | null = null;
			const result = await operation({ ...input, onConfirm: (c) => (confirm = c) });
			if (!getSigningKey()) return null;
			applyResult(result, success);
			if (result.status === 'uncertain') {
				pendingRetry = {
					event: result.event,
					confirm: confirm ?? (() => Promise.resolve(true)),
					success
				};
			}
			return result;
		} finally {
			opPending = false;
		}
	}

	async function retryPending(): Promise<void> {
		if (!pendingRetry || opPending) return;
		const input = mutationInput();
		if (!input) return;
		const pending = pendingRetry;
		opPending = true;
		opError = '';
		try {
			const result = await retryChannelMutation(input, pending.event, pending.confirm);
			if (!getSigningKey()) return;
			applyResult(result, pending.success);
			if (result.status !== 'uncertain') pendingRetry = null;
		} finally {
			opPending = false;
		}
	}

	function startEditing(): void {
		if (!channelState) return;
		editName = channelState.name;
		editDescription = channelState.description;
		editing = true;
		opError = '';
		opNotice = '';
	}

	async function saveEdits(): Promise<void> {
		const name = editName.trim();
		if (!name) {
			opError = 'Channel name is required.';
			return;
		}
		if (name.startsWith('#')) {
			opError = 'Channel names must not start with #.';
			return;
		}
		if (!channelState) return;
		const unchanged =
			name === channelState.name && editDescription.trim() === channelState.description;
		if (unchanged) {
			editing = false;
			return;
		}
		const result = await run(
			(input) =>
				editChannel(input, channelId, {
					name,
					description: editDescription.trim()
				}),
			'Channel details updated.'
		);
		// Only a relay-confirmed edit closes the editor. Any other outcome keeps
		// it open with the user's input intact and the failure surfaced inside
		// it, so the user can retry or fix the input instead of losing it.
		if (result?.status === 'confirmed') editing = false;
	}

	async function addMember(): Promise<void> {
		const target = addPubkey.trim().toLowerCase();
		if (!/^[0-9a-f]{64}$/.test(target)) {
			opError = 'Enter the member’s 64-character hex pubkey.';
			return;
		}
		const result = await run(
			(input) => addChannelMember(input, channelId, target),
			'Member added.'
		);
		// Clear the field only on confirmation; a rejection, transport failure,
		// or unconfirmed outcome keeps the entered pubkey for retry.
		if (result?.status === 'confirmed') addPubkey = '';
	}

	onMount(() => {
		void refresh();
		const timer = setInterval(() => void refresh(), 15_000);
		const reconnect = () => void refresh();
		const offline = () => {
			generation++;
			channelState = null;
			onPostingChange(false);
			error = 'Offline — posting disabled.';
		};
		window.addEventListener('online', reconnect);
		window.addEventListener('offline', offline);
		return () => {
			generation++;
			clearInterval(timer);
			window.removeEventListener('online', reconnect);
			window.removeEventListener('offline', offline);
		};
	});
</script>

<div class="border-b border-base-300 px-4 py-2 text-sm">
	<div class="flex flex-wrap items-center gap-3">
		<button
			type="button"
			class="btn btn-ghost btn-xs"
			aria-expanded={detailsOpen}
			onclick={() => (detailsOpen = !detailsOpen)}>Channel details</button
		>
		<button
			type="button"
			class="btn btn-ghost btn-xs"
			disabled={loading}
			onclick={() => void refresh()}>Refresh permissions</button
		>
		{#if loading}<span role="status">Checking channel permissions…</span>{/if}
		{#if error}<span role="alert" class="text-error">{error}</span>{/if}
		{#if !loading && !error && channelState && !canPostToChannel(channelState)}<span role="status"
				>Posting unavailable for your current channel role.</span
			>{/if}
	</div>
	{#if detailsOpen && channelState}
		{@const state = channelState}
		<div class="mt-2 space-y-3">
			<section aria-label="Channel information">
				<p>Private channels use relay access controls; they are not end-to-end encrypted.</p>
				<p class="font-semibold">
					{state.name} · {state.visibility}{state.archived ? ' · Archived' : ''}
				</p>
				<p class="whitespace-pre-wrap">{state.description || 'No description'}</p>
				<p>Your role: {state.role ?? 'Not a member'}</p>
				<p class="text-base-content/60">Metadata confirmed from relay-signed channel state.</p>
				{#if state.archived}
					<p role="status">This channel is archived; management actions are disabled.</p>
				{:else}
					<div class="flex flex-wrap items-center gap-2">
						{#if canJoinChannel(state, state.role)}
							<button
								type="button"
								class="btn btn-xs"
								disabled={opPending}
								onclick={() =>
									void run((input) => joinChannel(input, channelId), 'Joined channel.')}
								>Join channel</button
							>
						{/if}
						{#if state.role !== null}
							{#if isLastOwner(state.members, pubkey)}
								<span class="text-base-content/60"
									>Transfer ownership before leaving — a channel cannot lose its last owner.</span
								>
							{:else}
								<button
									type="button"
									class="btn btn-ghost btn-xs"
									disabled={opPending}
									onclick={() =>
										void run((input) => leaveChannel(input, channelId), 'Left channel.')}
									>Leave channel</button
								>
							{/if}
						{/if}
						{#if canEditChannelMetadata(state.role) && !editing}
							<button type="button" class="btn btn-ghost btn-xs" onclick={startEditing}
								>Edit channel</button
							>
						{/if}
					</div>
				{/if}
			</section>

			{#if editing}
				<section aria-label="Edit channel" class="space-y-2">
					<div>
						<label class="label text-xs" for="channel-edit-name">Name</label>
						<input
							id="channel-edit-name"
							class="input-bordered input w-full max-w-md input-sm"
							aria-label="Channel name"
							bind:value={editName}
						/>
					</div>
					<div>
						<label class="label text-xs" for="channel-edit-description">Description</label>
						<textarea
							id="channel-edit-description"
							class="textarea-bordered textarea w-full max-w-md"
							aria-label="Channel description"
							rows={2}
							bind:value={editDescription}></textarea>
					</div>
					<div class="flex gap-2">
						<button
							type="button"
							class="btn btn-primary btn-xs"
							disabled={opPending}
							onclick={() => void saveEdits()}>Save changes</button
						>
						<button
							type="button"
							class="btn btn-ghost btn-xs"
							disabled={opPending}
							onclick={() => (editing = false)}>Cancel</button
						>
					</div>
				</section>
			{/if}

			<section aria-label="Channel members">
				<h3 class="text-xs font-bold tracking-wider text-base-content/50 uppercase">Members</h3>
				<ul class="max-h-48 overflow-y-auto" aria-label="Channel members">
					{#each state.members as member (member.pubkey)}
						<li class="flex gap-2">
							<span class="truncate font-mono" title={member.pubkey}>{member.pubkey}</span><span
								>{member.role ?? 'Unknown role'}</span
							>
							{#if canRemoveMembers(state.role) && member.pubkey !== pubkey && !state.archived}
								<button
									type="button"
									class="text-error hover:underline"
									disabled={opPending}
									aria-label={`Remove member ${member.pubkey.slice(0, 8)}`}
									onclick={() =>
										void run(
											(input) => removeChannelMember(input, channelId, member.pubkey),
											'Member removed.'
										)}>remove</button
								>
							{/if}
						</li>
					{/each}
				</ul>
				{#if canAddMembers(state.role, state.visibility) && !state.archived}
					<div class="flex flex-wrap items-center gap-2">
						<input
							class="input-bordered input w-full max-w-md font-mono input-sm"
							aria-label="Member pubkey"
							placeholder="Member pubkey (64 hex characters)"
							bind:value={addPubkey}
						/>
						<button
							type="button"
							class="btn btn-xs"
							disabled={opPending || !/^[0-9a-fA-F]{64}$/.test(addPubkey.trim())}
							onclick={() => void addMember()}>Add member</button
						>
					</div>
				{/if}
			</section>

			{#if opPending}<p role="status">Waiting for the relay…</p>{/if}
			{#if opError}<p role="alert" class="text-error">{opError}</p>{/if}
			{#if opNotice}<p role="status" class="text-success">{opNotice}</p>{/if}
			{#if pendingRetry && !opPending}
				<div role="status" class="text-warning">
					<p>The operation’s acceptance is uncertain. Retry only the same signed request.</p>
					<button type="button" class="btn btn-sm" onclick={() => void retryPending()}
						>Retry same request</button
					>
				</div>
			{/if}
		</div>
	{/if}
</div>
