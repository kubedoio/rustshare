<script lang="ts">
	import { onMount } from 'svelte';
	import { queryRelayEvents } from '$lib/chat/nostr';
	import { getSigningKey } from '$lib/chat/session';
	import {
		parseChannelState,
		canPostToChannel,
		type ChatChannelState
	} from '$lib/chat/capabilities';

	interface Props {
		relayUrl: string;
		relayPubkey: string;
		channelId: string;
		pubkey: string;
		onPostingChange: (allowed: boolean) => void;
	}
	let { relayUrl, relayPubkey, channelId, pubkey, onPostingChange }: Props = $props();
	let channelState = $state<ChatChannelState | null>(null);
	let loading = $state(true);
	let error = $state('');
	let detailsOpen = $state(false);
	let generation = 0;

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
		<p>Private channels use relay access controls; they are not end-to-end encrypted.</p>
		<p class="font-semibold">
			{channelState.name} · {channelState.visibility}{channelState.archived ? ' · Archived' : ''}
		</p>
		<p class="whitespace-pre-wrap">{channelState.description || 'No description'}</p>
		<p>Your role: {channelState.role ?? 'Not a member'}</p>
		<ul aria-label="Channel members" class="max-h-48 overflow-y-auto">
			{#each channelState.members as member (member.pubkey)}
				<li class="flex gap-2">
					<span class="truncate font-mono" title={member.pubkey}>{member.pubkey}</span><span
						>{member.role ?? 'Unknown role'}</span
					>
				</li>
			{/each}
		</ul>
	{/if}
</div>
