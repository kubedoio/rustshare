<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import {
		requestDevicePairing,
		pollDevicePairing,
		getDeviceQrInfo,
		type DevicePollResponse,
		type DeviceQrInfoResponse
	} from '$lib/api/auth';
	import { authStore } from '$lib/stores/auth';
	import Toast from '$lib/components/common/Toast.svelte';
	import QrScanner from './QrScanner.svelte';
	import QRCode from 'qrcode';

	// Pairing state
	let userCode = '';
	let deviceCode = '';
	let expiresIn = 0;
	let countdown = 0;
	let isLoading = true;
	let isPolling = false;
	let errorMessage = '';
	let showError = false;

	// UI state
	let activeTab: 'qr' | 'key' | 'scan' = 'qr';
	let qrDataUrl = '';
	let qrInfo: DeviceQrInfoResponse | null = null;
	let verificationUriComplete = '';
	let pollInterval: ReturnType<typeof setInterval> | null = null;
	let countdownInterval: ReturnType<typeof setInterval> | null = null;

	onMount(() => {
		void (async () => {
			// Check if we're in scan mode (from query param)
			const mode = $page.url.searchParams.get('mode');
			if (mode === 'scan') {
				activeTab = 'scan';
			}

			await startPairing();
		})();
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
			// Get pairing codes
			const response = await requestDevicePairing();
			userCode = response.user_code;
			deviceCode = response.device_code;
			expiresIn = response.expires_in;
			countdown = expiresIn;
			verificationUriComplete = response.verification_uri_complete;

			// Generate QR code
			await generateQrCode();

			// Start polling for approval
			startPolling();
			startCountdown();
		} catch (error: any) {
			errorMessage = error.message || 'Failed to start device pairing. Please try again.';
			showError = true;
		} finally {
			isLoading = false;
		}
	}

	async function generateQrCode() {
		try {
			const pairingUrl = resolvePairingUrl();
			if (!pairingUrl) {
				qrInfo = await getDeviceQrInfo();
			}
			const resolvedPairingUrl = pairingUrl || buildFallbackPairingUrl();

			qrDataUrl = await QRCode.toDataURL(resolvedPairingUrl, {
				width: 280,
				margin: 2,
				color: {
					dark: '#000000',
					light: '#ffffff'
				},
				errorCorrectionLevel: 'M'
			});
		} catch (error) {
			console.error('Failed to generate QR code:', error);
		}
	}

	function resolvePairingUrl(): string {
		if (verificationUriComplete) {
			return verificationUriComplete;
		}

		// Older servers may only provide the instance base URL, so keep a fallback.
		if (qrInfo?.instance_url) {
			return buildFallbackPairingUrl();
		}

		return '';
	}

	function buildFallbackPairingUrl(): string {
		if (!qrInfo?.instance_url) return '';
		const path = qrInfo.device_pairing_path || '/device/approve';
		const normalizedPath = path.startsWith('/') ? path : `/${path}`;
		return `${qrInfo.instance_url}${normalizedPath}?device_code=${deviceCode}`;
	}

	function startPolling() {
		isPolling = true;
		pollInterval = setInterval(async () => {
			try {
				const response = await pollDevicePairing(deviceCode);
				handlePollResponse(response);
			} catch (error: any) {
				// Silently ignore poll errors
				console.warn('Pairing poll failed:', error);
			}
		}, 3000); // Poll every 3 seconds for faster response
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

	function formatTime(seconds: number): string {
		const mins = Math.floor(seconds / 60);
		const secs = seconds % 60;
		return `${mins}:${secs.toString().padStart(2, '0')}`;
	}

	async function handlePollResponse(response: DevicePollResponse) {
		if (response.status === 'approved') {
			stopPolling();

			// Store the token
			if (typeof window !== 'undefined') {
				window.sessionStorage.setItem('rustshare.websocket_token', response.token);
			}

			// Refresh auth and redirect
			try {
				await authStore.refreshSession();
				goto('/files');
			} catch (error) {
				goto('/files');
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
		qrDataUrl = '';
	}

	function handleRetry() {
		startPairing();
	}

	function handleScanSuccess(url: string) {
		try {
			const urlObj = new URL(url, window.location.origin);
			// Only allow same-origin URLs for security
			if (urlObj.origin === window.location.origin) {
				goto(urlObj.pathname + urlObj.search);
			} else {
				errorMessage = 'Invalid QR code: URL must be from this RustShare instance';
				showError = true;
			}
		} catch {
			if (url.startsWith('/')) {
				goto(url);
			} else {
				errorMessage = 'Invalid QR code: not a valid URL';
				showError = true;
			}
		}
	}

	function handleScanError(error: string) {
		showError = true;
		errorMessage = error;
	}

	function copyPairingKey() {
		navigator.clipboard.writeText(formatUserCode(userCode));
	}

	function formatUserCode(code: string): string {
		if (!code) return '';
		if (code.length === 8) {
			return `${code.slice(0, 4)}-${code.slice(4)}`;
		}
		return code;
	}

	function getProgressPercent(): number {
		return (countdown / expiresIn) * 100;
	}
</script>

<svelte:head>
	<title>Pair Device - RustShare</title>
	<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1" />
</svelte:head>

<div class="flex min-h-screen items-center justify-center bg-base-200 p-4">
	<div class="w-full max-w-md">
		<!-- Header -->
		<div class="mb-6 text-center">
			<div class="mb-4 inline-flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/10">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					class="h-8 w-8 text-primary"
					fill="none"
					viewBox="0 0 24 24"
					stroke="currentColor"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="1.5"
						d="M12 18v-5.25m0 0a6.01 6.01 0 001.5-.189m-1.5.189a6.01 6.01 0 01-1.5-.189m3.75 7.478a12.06 12.06 0 01-4.5 0m3.75 2.383a14.406 14.406 0 01-3 0M14.25 18v-.192c0-.983.658-1.823 1.508-2.316a7.5 7.5 0 10-7.517 0c.85.493 1.509 1.333 1.509 2.316V18"
					/>
				</svg>
			</div>
			<h1 class="mb-1 text-2xl font-bold">Pair Your Device</h1>
			<p class="text-sm text-base-content/60">Connect this device to your RustShare account</p>
		</div>

		{#if isLoading}
			<!-- Loading State -->
			<div class="card bg-base-100 shadow-xl">
				<div class="card-body items-center py-12">
					<span class="loading mb-4 loading-lg loading-spinner text-primary"></span>
					<p class="text-base-content/60">Generating pairing code...</p>
				</div>
			</div>
		{:else if showError}
			<!-- Error State -->
			<div class="card bg-base-100 shadow-xl">
				<div class="card-body items-center py-8 text-center">
					<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-error/10">
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="h-8 w-8 text-error"
							fill="none"
							viewBox="0 0 24 24"
							stroke="currentColor"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M12 9v3.75m9-.75a9 9 0 11-18 0 9 9 0 0118 0zm-9 3.75h.008v.008H12v-.008z"
							/>
						</svg>
					</div>
					<h2 class="mb-2 text-lg font-semibold">Pairing Failed</h2>
					<p class="mb-6 text-sm text-base-content/60">{errorMessage}</p>
					<button class="btn w-full btn-primary" onclick={handleRetry}> Try Again </button>
				</div>
			</div>
		{:else}
			<!-- Main Pairing Card -->
			<div class="card overflow-hidden bg-base-100 shadow-xl">
				<!-- Tabs -->
				<div class="tabs-boxed tabs rounded-none bg-base-200 p-2">
					<button
						class="tab flex-1 {activeTab === 'qr' ? 'tab-active bg-base-100 shadow-sm' : ''}"
						onclick={() => (activeTab = 'qr')}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="mr-2 h-4 w-4"
							fill="none"
							viewBox="0 0 24 24"
							stroke="currentColor"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M3.75 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 013.75 9.375v-4.5zM3.75 14.625c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5a1.125 1.125 0 01-1.125-1.125v-4.5zM13.5 4.875c0-.621.504-1.125 1.125-1.125h4.5c.621 0 1.125.504 1.125 1.125v4.5c0 .621-.504 1.125-1.125 1.125h-4.5A1.125 1.125 0 0113.5 9.375v-4.5z"
							/>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M6.75 6.75h.75v.75h-.75v-.75zM6.75 16.5h.75v.75h-.75v-.75zM16.5 6.75h.75v.75h-.75v-.75zM13.5 13.5h.75v.75h-.75v-.75zM13.5 19.5h.75v.75h-.75v-.75zM19.5 13.5h.75v.75h-.75v-.75zM19.5 19.5h.75v.75h-.75v-.75zM16.5 16.5h.75v.75h-.75v-.75z"
							/>
						</svg>
						Scan QR
					</button>
					<button
						class="tab flex-1 {activeTab === 'key' ? 'tab-active bg-base-100 shadow-sm' : ''}"
						onclick={() => (activeTab = 'key')}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="mr-2 h-4 w-4"
							fill="none"
							viewBox="0 0 24 24"
							stroke="currentColor"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"
							/>
						</svg>
						Pairing Key
					</button>
					<button
						class="tab flex-1 {activeTab === 'scan' ? 'tab-active bg-base-100 shadow-sm' : ''}"
						onclick={() => (activeTab = 'scan')}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							class="mr-2 h-4 w-4"
							fill="none"
							viewBox="0 0 24 24"
							stroke="currentColor"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M6.827 6.175A2.31 2.31 0 015.186 7.23c-.38.054-.757.112-1.134.175C2.999 7.58 2.25 8.507 2.25 9.574V18a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9.574c0-1.067-.75-1.994-1.802-2.169a47.865 47.865 0 00-1.134-.175 2.31 2.31 0 01-1.64-1.055l-.822-1.316a2.192 2.192 0 00-1.736-1.039 48.774 48.774 0 00-5.232 0 2.192 2.192 0 00-1.736 1.039l-.821 1.316z"
							/>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M16.5 12.75a4.5 4.5 0 11-9 0 4.5 4.5 0 019 0zM18.75 10.5h.008v.008h-.008V10.5z"
							/>
						</svg>
						Scan
					</button>
				</div>

				<div class="card-body p-6">
					{#if activeTab === 'qr'}
						<!-- QR Code Tab -->
						<div class="space-y-6 text-center">
							<div class="space-y-2">
								<h2 class="text-lg font-semibold">Scan with another device</h2>
								<p class="text-sm text-base-content/60">
									Open RustShare on an authenticated device and scan this code
								</p>
							</div>

							{#if qrDataUrl}
								<div class="flex justify-center">
									<div class="rounded-2xl bg-white p-4 shadow-lg">
										<img src={qrDataUrl} alt="Pairing QR Code" class="h-56 w-56" />
									</div>
								</div>
							{:else}
								<div class="flex justify-center py-8">
									<span class="loading loading-lg loading-spinner text-primary"></span>
								</div>
							{/if}

							<!-- Status -->
							<div class="space-y-3">
								<div class="flex items-center justify-center gap-2 text-sm text-base-content/60">
									<span class="relative flex h-3 w-3">
										<span
											class="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-75"
										></span>
										<span class="relative inline-flex h-3 w-3 rounded-full bg-success"></span>
									</span>
									<span>Waiting for approval...</span>
								</div>

								<!-- Progress bar -->
								<div class="mx-auto w-full max-w-xs space-y-1">
									<div class="flex justify-between text-xs text-base-content/50">
										<span>Expires in</span>
										<span class="font-mono">{formatTime(countdown)}</span>
									</div>
									<progress
										class="progress h-2 w-full progress-success"
										value={countdown}
										max={expiresIn}
									></progress>
								</div>
							</div>

							<!-- Help text -->
							<div class="pt-2 text-xs text-base-content/50">
								<p>
									Go to <strong>Settings → Devices</strong> on another device<br />and scan this QR
									code to approve
								</p>
							</div>
						</div>
					{:else if activeTab === 'key'}
						<!-- Pairing Key Tab -->
						<div class="space-y-6 text-center">
							<div class="space-y-2">
								<h2 class="text-lg font-semibold">Enter pairing key</h2>
								<p class="text-sm text-base-content/60">
									Enter this 8-character code on an authenticated RustShare session
								</p>
							</div>

							<!-- Pairing Key Display -->
							<div class="relative">
								<button
									class="group w-full rounded-2xl border-2 border-dashed border-base-300 bg-base-200 p-6 transition-colors hover:bg-base-300"
									onclick={copyPairingKey}
									title="Click to copy"
								>
									<div class="font-mono text-4xl font-bold tracking-[0.2em] text-primary">
										{formatUserCode(userCode)}
									</div>
									<div
										class="mt-2 flex items-center justify-center gap-1 text-xs text-base-content/50"
									>
										<svg
											xmlns="http://www.w3.org/2000/svg"
											class="h-3 w-3"
											fill="none"
											viewBox="0 0 24 24"
											stroke="currentColor"
										>
											<path
												stroke-linecap="round"
												stroke-linejoin="round"
												stroke-width="1.5"
												d="M15.666 3.888A2.25 2.25 0 0013.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 01-.75.75H9a.75.75 0 01-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 01-2.25 2.25H6.75A2.25 2.25 0 014.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 011.927-.184"
											/>
										</svg>
										Click to copy
									</div>
								</button>
							</div>

							<!-- Status -->
							<div class="space-y-3">
								<div class="flex items-center justify-center gap-2 text-sm text-base-content/60">
									<span class="relative flex h-3 w-3">
										<span
											class="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-75"
										></span>
										<span class="relative inline-flex h-3 w-3 rounded-full bg-success"></span>
									</span>
									<span>Waiting for approval...</span>
								</div>

								<!-- Progress bar -->
								<div class="mx-auto w-full max-w-xs space-y-1">
									<div class="flex justify-between text-xs text-base-content/50">
										<span>Expires in</span>
										<span class="font-mono">{formatTime(countdown)}</span>
									</div>
									<progress
										class="progress h-2 w-full progress-success"
										value={countdown}
										max={expiresIn}
									></progress>
								</div>
							</div>

							<!-- Help text -->
							<div class="pt-2 text-xs text-base-content/50">
								<p>
									Go to <strong>Settings → Devices</strong> on another device<br />and enter this
									code to approve pairing
								</p>
							</div>
						</div>
					{:else if activeTab === 'scan'}
						<!-- Scan Tab -->
						<div class="space-y-4 text-center">
							<div class="space-y-2">
								<h2 class="text-lg font-semibold">Scan QR code</h2>
								<p class="text-sm text-base-content/60">
									Scan a pairing QR code from another RustShare device
								</p>
							</div>

							<QrScanner
								onSuccess={handleScanSuccess}
								onError={handleScanError}
								onClose={() => (activeTab = 'qr')}
								inline={true}
							/>
						</div>
					{/if}
				</div>

				<!-- Footer -->
				<div class="border-t border-base-200 bg-base-200/50 px-6 py-4">
					<div class="flex items-center justify-between">
						<span class="text-xs text-base-content/50">
							Code expires in {formatTime(countdown)}
						</span>
						<button
							class="btn text-xs btn-ghost btn-sm"
							onclick={handleRetry}
							disabled={countdown > 30}
						>
							<svg
								xmlns="http://www.w3.org/2000/svg"
								class="mr-1 h-3 w-3"
								fill="none"
								viewBox="0 0 24 24"
								stroke="currentColor"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="1.5"
									d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"
								/>
							</svg>
							New Code
						</button>
					</div>
				</div>
			</div>

			<!-- Server Info -->
			{#if qrInfo}
				<div class="mt-4 text-center text-xs text-base-content/40">
					<p>{qrInfo.instance_url}</p>
				</div>
			{/if}
		{/if}
	</div>
</div>

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
