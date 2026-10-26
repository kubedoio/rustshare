<script lang="ts">
	import { apiClient } from '$lib/api/client';
	import {
		generateSecretKey,
		publicKeyOf,
		signEvent,
		buildUnsignedEvent,
		NOSTR_KIND_AUTH
	} from '$lib/chat/nostr';
	import {
		saveChatKey,
		hasChatKey,
		loadChatKey,
		exportChatKey,
		importChatKey,
		chatKeySession,
		assertChatKeySession
	} from '$lib/chat/keys';
	import { currentUser } from '$lib/stores/auth';

	interface Props {
		onBound: () => void;
	}

	let { onBound }: Props = $props();

	let busy = $state(false);
	let error = $state('');
	let notice = $state('');
	let passphrase = $state('');
	let mode = $state<'create' | 'import'>('create');
	let backupJson = $state('');
	let confirmLegacy = $state(false);
	let localKey = $state(hasChatKey());

	async function bind(): Promise<void> {
		if (busy) return;
		if (!passphrase) {
			error = 'choose a passphrase to encrypt your key';
			backupJson = '';
			confirmLegacy = false;
			return;
		}
		busy = true;
		error = '';
		const generation = chatKeySession();
		const user = $currentUser;
		const workspace = user?.tenant_id;
		const checkAccount = () => {
			assertChatKeySession(generation);
			if ($currentUser?.id !== user?.id || $currentUser?.tenant_id !== workspace)
				throw new Error('Chat account changed. Try again.');
		};
		try {
			if (!workspace) throw new Error('workspace id unavailable');
			const existing = hasChatKey();
			const secretKey =
				mode === 'import'
					? await importChatKey(backupJson, passphrase, undefined, confirmLegacy)
					: existing
						? await loadChatKey(passphrase)
						: generateSecretKey();
			checkAccount();
			const pubkey = publicKeyOf(secretKey);
			if (mode === 'create' && !existing) {
				await saveChatKey(secretKey, pubkey, passphrase);
			}
			checkAccount();
			localKey = true;
			const challenge: {
				challenge_id: string;
				nonce: string;
				buzz_pubkey: string;
				relay_url: string;
				expires_at: string;
			} = await apiClient.post('/applications/chat/identity-binding/challenge', {
				workspace_id: workspace,
				buzz_pubkey: pubkey
			});
			checkAccount();
			if (
				challenge.buzz_pubkey !== pubkey ||
				!Number.isFinite(Date.parse(challenge.expires_at)) ||
				Date.parse(challenge.expires_at) <= Date.now()
			)
				throw new Error('Invalid or expired binding challenge. Try again.');
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
			checkAccount();
			await apiClient.post('/applications/chat/identity-binding/verify', {
				challenge_id: challenge.challenge_id,
				event: auth
			});
			checkAccount();
			await apiClient.post('/applications/chat/admission', {
				workspace_id: workspace
			});
			checkAccount();
			notice = 'Bound and admission queued.';
			onBound();
		} catch (err) {
			error = err instanceof Error ? err.message : 'binding failed';
		} finally {
			passphrase = '';
			backupJson = '';
			confirmLegacy = false;
			localKey = hasChatKey();
			busy = false;
		}
	}
</script>

<div class="p-6">
	<h2 class="mb-2 text-lg font-semibold">Set up Chat</h2>
	<p class="mb-4 text-sm text-base-content/60">
		Chat messages are signed with a key held only in this browser. Choose a passphrase to encrypt
		it. Export it after setup — without a backup, another device cannot use the same identity.
		Elembra manages your account; Buzz verifies your signatures. Elembra cannot recover a lost key
		or forgotten passphrase. If every copy is lost, an administrator must explicitly rotate your
		identity; your original signing key cannot be restored.
	</p>
	<div class="mb-3 flex gap-2" role="group" aria-label="Chat identity setup">
		<button
			type="button"
			class="btn btn-sm"
			aria-pressed={mode === 'create'}
			disabled={busy}
			onclick={() => {
				mode = 'create';
				backupJson = '';
				passphrase = '';
			}}>{localKey ? 'Use stored Chat identity' : 'Create new Chat identity'}</button
		>
		<button
			type="button"
			class="btn btn-sm"
			aria-pressed={mode === 'import'}
			disabled={busy}
			onclick={() => {
				mode = 'import';
				passphrase = '';
			}}>Import existing Chat identity</button
		>
	</div>
	{#if mode === 'import'}
		<label for="chat-onboarding-backup" class="mb-1 block text-sm">Encrypted key backup</label>
		<textarea
			id="chat-onboarding-backup"
			class="textarea mb-2 w-full"
			maxlength={4096}
			bind:value={backupJson}></textarea>
		<label class="mb-3 flex items-start gap-2 text-sm"
			><input type="checkbox" bind:checked={confirmLegacy} disabled={busy} />For a legacy backup
			without account metadata, I confirm this is my Chat identity for this account. The server will
			still verify key possession and binding eligibility.</label
		>
	{/if}
	<div class="mb-2 flex gap-2">
		<input
			type="password"
			class="input input-sm"
			placeholder="key passphrase"
			aria-label={mode === 'import' ? 'backup passphrase' : 'key passphrase'}
			bind:value={passphrase}
		/>
		<button type="button" class="btn btn-primary btn-sm" disabled={busy} onclick={bind}>
			{busy
				? 'Binding…'
				: mode === 'import'
					? 'Import & bind'
					: localKey
						? 'Unlock & bind stored identity'
						: 'Create identity & bind'}
		</button>
	</div>
	{#if localKey}
		<button
			type="button"
			class="btn btn-sm"
			onclick={async () => {
				try {
					await navigator.clipboard.writeText(exportChatKey());
					notice = 'Encrypted backup copied. Keep it and your passphrase in separate safe places.';
				} catch {
					error = 'Could not copy the backup. Allow clipboard access and try again.';
				}
			}}
		>
			Export key backup
		</button>
	{/if}
	{#if notice}<p role="status" class="mt-2 text-sm text-success">{notice}</p>{/if}
	{#if error}<p role="alert" class="mt-2 text-sm text-error">{error}</p>{/if}
</div>
