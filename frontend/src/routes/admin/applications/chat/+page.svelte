<script lang="ts">
	import { createQuery, createMutation } from '$lib/query-compat';
	import {
		provisionChatCommunity,
		getChatCommunityMapping,
		connectChatCommunity,
		getChatStatus,
		type ChatProvisionResult,
		type AdminChatCommunityMapping
	} from '$lib/api/chat';
	import { currentUser } from '$lib/stores/auth';
	import { ApiError } from '$lib/api/types';

	// The admin layout only renders once the session user is loaded, and a
	// tenant id never changes mid-session, so capturing it once is safe.
	const workspaceId = $currentUser?.tenant_id ?? '';

	const mappingQuery = createQuery<AdminChatCommunityMapping | null>({
		queryKey: ['admin-chat-mapping', workspaceId],
		queryFn: async () => {
			try {
				return await getChatCommunityMapping(workspaceId);
			} catch (err) {
				if (err instanceof ApiError && err.status === 404) return null;
				throw err;
			}
		},
		enabled: workspaceId !== ''
	});

	const statusQuery = createQuery({
		queryKey: ['chat-status'],
		queryFn: () => getChatStatus(),
		enabled: workspaceId !== ''
	});

	let provisionResult = $state<ChatProvisionResult | null>(null);
	let provisionError = $state('');

	const provisionMutation = createMutation<ChatProvisionResult, Error, string>({
		mutationFn: (id: string) => provisionChatCommunity(id),
		onSuccess: (result) => {
			provisionResult = result;
			provisionError = '';
			mappingQuery.refetch();
		},
		onError: (err: Error) => {
			provisionError = err.message;
		}
	});

	let showConnectForm = $state(false);
	let relayUrl = $state('');
	let communityId = $state('');
	let relayPubkey = $state('');
	let connectError = $state('');

	const connectMutation = createMutation<
		void,
		Error,
		{ community_id: string; relay_url: string; relay_pubkey?: string }
	>({
		mutationFn: (body) => connectChatCommunity(workspaceId, body),
		onSuccess: () => {
			showConnectForm = false;
			relayUrl = '';
			communityId = '';
			relayPubkey = '';
			connectError = '';
			mappingQuery.refetch();
		},
		onError: (err: Error) => {
			connectError = err.message;
		}
	});

	const mapping = $derived($mappingQuery.data);
	const chatEnabled = $derived($statusQuery.data?.chat_enabled ?? false);
</script>

<svelte:head>
	<title>Chat Settings - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-6xl">
	<div class="mb-6">
		<h1 class="text-2xl font-semibold text-base-content">Chat Settings</h1>
		<p class="mt-1 text-sm text-base-content/60">
			Configure which community this workspace's Chat connects to.
		</p>
	</div>

	<div class="grid gap-6 lg:grid-cols-2">
		<section class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="text-lg font-semibold text-base-content">Status</h2>
			{#if $statusQuery.isLoading}
				<div class="loading loading-spinner loading-md mt-4 text-brand-500"></div>
			{:else if chatEnabled}
				<p class="mt-2 text-sm text-success">Chat is enabled for this workspace.</p>
			{:else}
				<p class="mt-2 text-sm text-base-content/70">
					Chat is not enabled.
					<a href="/admin/applications" class="text-primary">Open Applications</a>
					to enable it.
				</p>
			{/if}

			<h3 class="mt-6 text-sm font-semibold tracking-wider text-base-content/60 uppercase">
				Community mapping
			</h3>
			{#if $mappingQuery.isLoading}
				<div class="loading loading-spinner loading-md mt-4 text-brand-500"></div>
			{:else if $mappingQuery.isError}
				<p class="mt-2 text-sm text-error">Could not load the current community mapping.</p>
			{:else if mapping}
				<dl class="mt-3 space-y-2 text-sm">
					<div class="flex gap-2">
						<dt class="w-32 shrink-0 text-base-content/60">community_id</dt>
						<dd class="font-mono text-base-content">{mapping.community_id}</dd>
					</div>
					<div class="flex gap-2">
						<dt class="w-32 shrink-0 text-base-content/60">relay_url</dt>
						<dd class="font-mono text-base-content">{mapping.relay_url}</dd>
					</div>
					<div class="flex gap-2">
						<dt class="w-32 shrink-0 text-base-content/60">relay_pubkey</dt>
						<dd class="font-mono text-base-content">{mapping.relay_pubkey ?? '-'}</dd>
					</div>
					<div class="flex gap-2">
						<dt class="w-32 shrink-0 text-base-content/60">active</dt>
						<dd class="text-base-content">{mapping.active ? 'Yes' : 'No'}</dd>
					</div>
				</dl>
			{:else}
				<p class="mt-2 text-sm text-base-content/60">Chat is not yet connected to a community.</p>
			{/if}
		</section>

		<section class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="text-lg font-semibold text-base-content">Configuration</h2>
			<p class="mt-1 text-sm text-base-content/60">
				Existing mappings are never overwritten automatically.
			</p>

			<button
				type="button"
				class="btn btn-primary mt-4"
				disabled={workspaceId === '' || $provisionMutation.isPending}
				onclick={() => {
					// Errors are surfaced via onError → provisionError; swallow the
					// dropped mutate() promise so it never counts as unhandled.
					$provisionMutation.mutate(workspaceId).catch(() => {});
				}}
			>
				{#if $provisionMutation.isPending}
					<span class="loading loading-spinner loading-sm"></span>
				{/if}
				Set up automatically
			</button>
			{#if provisionResult}
				<p class="mt-3 text-sm text-success" role="status">
					Connected to community {provisionResult.community_id} ({provisionResult.status}).
				</p>
			{/if}
			{#if provisionError}
				<p class="mt-3 text-sm text-error" role="alert">{provisionError}</p>
			{/if}

			<button
				type="button"
				class="btn btn-outline mt-4"
				onclick={() => (showConnectForm = !showConnectForm)}
			>
				Connect existing Chat deployment
			</button>
			{#if showConnectForm}
				<form
					class="mt-4 space-y-3"
					onsubmit={(e) => {
						e.preventDefault();
						connectError = '';
						$connectMutation
							.mutate({
								community_id: communityId.trim(),
								relay_url: relayUrl.trim(),
								...(relayPubkey.trim() ? { relay_pubkey: relayPubkey.trim() } : {})
							})
							.catch(() => {});
					}}
				>
					<label class="block text-sm">
						<span class="text-base-content/70">relay_url (ws/wss)</span>
						<input
							class="input input-bordered mt-1 w-full"
							type="text"
							placeholder="wss://relay.example"
							bind:value={relayUrl}
						/>
					</label>
					<label class="block text-sm">
						<span class="text-base-content/70">community_id</span>
						<input
							class="input input-bordered mt-1 w-full"
							type="text"
							placeholder="00000000-0000-0000-0000-000000000000"
							bind:value={communityId}
						/>
					</label>
					<label class="block text-sm">
						<span class="text-base-content/70">relay_pubkey (optional)</span>
						<input
							class="input input-bordered mt-1 w-full"
							type="text"
							placeholder="npub…"
							bind:value={relayPubkey}
						/>
					</label>
					<button
						type="submit"
						class="btn btn-primary"
						disabled={!relayUrl.trim() || !communityId.trim() || $connectMutation.isPending}
					>
						{#if $connectMutation.isPending}
							<span class="loading loading-spinner loading-sm"></span>
						{/if}
						Connect
					</button>
					{#if connectError}
						<p class="text-sm text-error" role="alert">{connectError}</p>
					{/if}
				</form>
			{/if}
		</section>
	</div>
</div>
