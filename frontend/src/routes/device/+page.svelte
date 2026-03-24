<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { goto } from '$app/navigation';
	import { requestDevicePairing, pollDevicePairing, type DevicePollResponse } from '$lib/api/auth';
	import { authStore } from '$lib/stores/auth';
	import Toast from '$lib/components/common/Toast.svelte';

	let userCode = '';
	let deviceCode = '';
	let expiresIn = 0;
	let isLoading = true;
	let isPolling = false;
	let errorMessage = '';
	let showError = false;
	let countdown = 0;
	let pollInterval: any;
	let countdownInterval: any;

	onMount(async () => {
		await startPairing();
	});

	onDestroy(() => {
		stopPolling();
	});

	async function startPairing() {
		isLoading = true;
		errorMessage = '';
		showError = false;
		stopPolling();

		try {
			const response = await requestDevicePairing();
			userCode = response.user_code;
			deviceCode = response.device_code;
			expiresIn = response.expires_in;
			countdown = expiresIn;

			startPolling();
			startCountdown();
		} catch (error: any) {
			errorMessage = error.message || 'Failed to start device pairing. Please try again.';
			showError = true;
		} finally {
			isLoading = false;
		}
	}

	function startPolling() {
		isPolling = true;
		pollInterval = setInterval(async () => {
			try {
				const response = await pollDevicePairing(deviceCode);
				handlePollResponse(response);
			} catch (error: any) {
				// Silently ignore poll errors (network issues, rate limits)
				// Rate limit (429) is handled by the PollRateLimiter on the backend
				console.warn('Pairing poll failed:', error);
			}
		}, 5000); // Poll every 5 seconds
	}

	function stopPolling() {
		isPolling = false;
		if (pollInterval) clearInterval(pollInterval);
		if (countdownInterval) clearInterval(countdownInterval);
	}

	function startCountdown() {
		countdownInterval = setInterval(() => {
			if (countdown > 0) {
				countdown--;
			} else {
				handleExpired();
			}
		}, 1000);
	}

	async function handlePollResponse(response: DevicePollResponse) {
		if (response.status === 'approved') {
			stopPolling();
			
			// Store the token in sessionStorage (used by ApiClient and WebSocket)
			if (typeof window !== 'undefined') {
				window.sessionStorage.setItem('rustshare.websocket_token', response.token);
			}
			
			// Refresh auth store profile and redirect
			try {
				await authStore.refreshSession();
				goto('/files');
			} catch (error) {
				console.error('Failed to load profile after pairing:', error);
				goto('/files'); // Try to go anyway
			}
		} else if (response.status === 'expired') {
			handleExpired();
		}
	}

	function handleExpired() {
		stopPolling();
		errorMessage = 'Pairing code has expired. Please try again.';
		showError = true;
		userCode = '';
	}

	function formatUserCode(code: string) {
		if (!code) return '';
		if (code.length === 8) {
			return `${code.slice(0, 4)}-${code.slice(4)}`;
		}
		return code;
	}

	function handleRetry() {
		startPairing();
	}
</script>

<svelte:head>
	<title>Pair Device - RustShare</title>
</svelte:head>

<div class="bg-base-200 flex min-h-screen items-center justify-center p-4">
	<div class="card w-full max-w-md bg-base-100 shadow-xl">
		<div class="card-body items-center text-center">
			<h2 class="card-title text-2xl mb-2">RustShare</h2>
			<p class="text-base-content/70 mb-6">Pair your device</p>

			{#if isLoading}
				<div class="flex flex-col items-center py-8">
					<span class="loading loading-spinner loading-lg text-primary"></span>
					<p class="mt-4 text-sm opacity-70">Generating code...</p>
				</div>
			{:else if userCode}
				<div class="w-full space-y-8">
					<div class="space-y-2">
						<p class="text-sm font-medium">Enter this code on your other device:</p>
						<div class="flex justify-center">
							<div class="bg-base-200 text-primary font-mono text-4xl font-bold tracking-widest py-6 px-8 rounded-lg border-2 border-primary/20 shadow-inner">
								{formatUserCode(userCode)}
							</div>
						</div>
					</div>

					<div class="flex flex-col items-center gap-4 py-4">
						<div class="flex items-center gap-3">
							<span class="loading loading-ring loading-md text-primary"></span>
							<span class="text-sm font-medium">Waiting for approval...</span>
						</div>
						
						<div class="w-full max-w-xs space-y-1">
							<div class="flex justify-between text-xs opacity-60">
								<span>Expires in</span>
								<span>{Math.floor(countdown / 60)}:{(countdown % 60).toString().padStart(2, '0')}</span>
							</div>
							<progress 
								class="progress progress-primary w-full" 
								value={countdown} 
								max={expiresIn}
							></progress>
						</div>
					</div>

					<div class="card-actions justify-center">
						<button class="btn btn-ghost btn-sm" on:click={handleRetry}>
							Cancel and restart
						</button>
					</div>
				</div>
			{:else if showError}
				<div class="alert alert-error mb-6">
					<svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
					<span>{errorMessage}</span>
				</div>
				<button class="btn btn-primary w-full" on:click={handleRetry}>
					Try Again
				</button>
			{/if}

			<div class="mt-8 text-xs text-base-content/50 max-w-xs mx-auto">
				<p>To pair, log in to RustShare on another device, go to Settings > Devices, and enter the code shown above.</p>
			</div>
		</div>
	</div>
</div>

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
