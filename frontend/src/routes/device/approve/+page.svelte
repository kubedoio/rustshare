<script lang="ts">
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { approveDevicePairingByDeviceCode } from '$lib/api/auth';
	import { ApiError } from '$lib/api/types';
	import { authStore } from '$lib/stores/auth';

	let deviceCode = '';
	let isSubmitting = false;
	let hasRedirectedToLogin = false;
	let state: 'loading' | 'invalid' | 'ready' | 'success' | 'error' = 'loading';
	let errorMessage = '';

	$: deviceCode = $page.url.searchParams.get('device_code')?.trim() ?? '';

	$: if (browser) {
		if (!deviceCode) {
			state = 'invalid';
		} else if (!$authStore.isLoading && !$authStore.isAuthenticated && !hasRedirectedToLogin) {
			hasRedirectedToLogin = true;
			const redirectTarget = `${$page.url.pathname}${$page.url.search}`;
			goto(`/login?redirect_to=${encodeURIComponent(redirectTarget)}`);
		} else if (!$authStore.isLoading && $authStore.isAuthenticated && state === 'loading') {
			state = 'ready';
		}
	}

	async function handleApprove() {
		if (!deviceCode) {
			state = 'invalid';
			return;
		}

		isSubmitting = true;
		errorMessage = '';

		try {
			await approveDevicePairingByDeviceCode(deviceCode);
			state = 'success';
		} catch (error) {
			state = 'error';
			errorMessage =
				error instanceof ApiError && error.status === 404
					? 'This approval link is invalid or has expired. Start a new pairing request from the desktop client.'
					: error instanceof Error
						? error.message
						: 'Failed to approve device pairing.';
		} finally {
			isSubmitting = false;
		}
	}
</script>

<svelte:head>
	<title>Approve Device - RustShare</title>
</svelte:head>

<div class="min-h-screen bg-base-200 flex items-center justify-center p-4">
	<div class="w-full max-w-lg rounded-2xl border border-base-300 bg-base-100 p-8 shadow-xl">
		<h1 class="text-2xl font-bold text-base-content">Approve Device Pairing</h1>

		{#if state === 'loading'}
			<p class="mt-4 text-sm text-base-content/70">Checking your session and validating the approval link...</p>
		{:else if state === 'invalid'}
			<p class="mt-4 text-sm text-error">
				This approval link is missing its device token. Start the pairing flow again from the desktop client.
			</p>
		{:else if state === 'ready'}
			<div class="mt-4 space-y-4">
				<p class="text-sm text-base-content/75">
					Approve this device only if you started the pairing flow yourself. The desktop client is waiting for confirmation.
				</p>
				<p class="rounded-lg bg-base-200 px-4 py-3 text-sm text-base-content/70">
					This approval link is valid for 5 minutes and should be opened from an authenticated RustShare web UI session.
				</p>
				<button
					type="button"
					class="btn btn-primary"
					on:click={handleApprove}
					disabled={isSubmitting}
				>
					{#if isSubmitting}Approving...{:else}Approve Device{/if}
				</button>
			</div>
		{:else if state === 'success'}
			<div class="mt-4 space-y-3">
				<p class="text-sm text-success">Device approved. You can return to the desktop client and continue setup.</p>
			</div>
		{:else}
			<p class="mt-4 text-sm text-error">{errorMessage}</p>
		{/if}
	</div>
</div>
