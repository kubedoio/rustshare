<script lang="ts">
	import { apiClient } from '$lib/api/client';
	import {
		generateSecretKey,
		publicKeyOf,
		signEvent,
		buildUnsignedEvent,
		NOSTR_KIND_AUTH
	} from '$lib/chat/nostr';
	import { saveChatKey, hasChatKey, loadChatKey, exportChatKey } from '$lib/chat/keys';
	import { currentUser } from '$lib/stores/auth';

	interface Props {
		onBound: () => void;
	}

	let { onBound }: Props = $props();

	let busy = $state(false);
	let error = $state('');
	let notice = $state('');
	let passphrase = $state('');

	async function bind(): Promise<void> {
		if (!passphrase) {
			error = 'choose a passphrase to encrypt your key';
			return;
		}
		busy = true;
		error = '';
		try {
			const secretKey = hasChatKey() ? await loadChatKey(passphrase) : generateSecretKey();
			const pubkey = publicKeyOf(secretKey);
			if (!hasChatKey()) {
				await saveChatKey(secretKey, pubkey, passphrase);
			}
			const challenge: {
				challenge_id: string;
				nonce: string;
				buzz_pubkey: string;
				relay_url: string;
				expires_at: string;
			} = await apiClient.post('/applications/chat/identity-binding/challenge', {
				workspace_id: workspaceId(),
				buzz_pubkey: pubkey
			});
			const auth = await signEvent(
				await buildUnsignedEvent(
					NOSTR_KIND_AUTH,
					'',
					[
						['relay', challenge.relay_url],
						['challenge', challenge.nonce]
					],
					pubkey
				),
				secretKey
			);
			await apiClient.post('/applications/chat/identity-binding/verify', {
				challenge_id: challenge.challenge_id,
				event: auth
			});
			await apiClient.post('/applications/chat/admission', {
				workspace_id: workspaceId()
			});
			notice = 'Bound and admission queued.';
			onBound();
		} catch (err) {
			error = err instanceof Error ? err.message : 'binding failed';
		} finally {
			busy = false;
		}
	}

	function workspaceId(): string {
		// Workspace id == tenant id in this deployment (see backend handlers,
		// e.g. PrincipalContext::user(.., WorkspaceId(auth.tenant_id))).
		const user = $currentUser;
		if (user?.tenant_id) return user.tenant_id;
		throw new Error('workspace id unavailable');
	}
</script>

<div class="p-6">
	<h2 class="mb-2 text-lg font-semibold">Set up Chat</h2>
	<p class="mb-4 text-sm text-base-content/60">
		Chat messages are signed with a key held only in this browser. Choose a passphrase to encrypt
		it. Export it after setup — without a backup, another device cannot use the same identity.
	</p>
	<div class="mb-2 flex gap-2">
		<input
			type="password"
			class="input input-sm"
			placeholder="key passphrase"
			bind:value={passphrase}
		/>
		<button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={bind}>
			{busy ? 'Binding…' : 'Generate key & bind'}
		</button>
	</div>
	{#if hasChatKey()}
		<button
			type="button"
			class="btn btn-sm"
			onclick={() => {
				const backup = exportChatKey();
				navigator.clipboard.writeText(backup);
				notice = 'Backup copied to clipboard.';
			}}
		>
			Export key backup
		</button>
	{/if}
	{#if notice}<p class="mt-2 text-sm text-success">{notice}</p>{/if}
	{#if error}<p class="mt-2 text-sm text-error">{error}</p>{/if}
</div>
