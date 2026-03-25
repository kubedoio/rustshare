<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { getDeviceQrInfo, type DeviceQrInfoResponse } from '$lib/api/auth';
	import QRCode from 'qrcode';
	import Toast from '$lib/components/common/Toast.svelte';

	let qrInfo: DeviceQrInfoResponse | null = null;
	let qrDataUrl = '';
	let isLoading = true;
	let errorMessage = '';
	let showError = false;

	onMount(async () => {
		await loadQrInfo();
	});

	async function loadQrInfo() {
		isLoading = true;
		errorMessage = '';
		showError = false;

		try {
			qrInfo = await getDeviceQrInfo();
			const fullUrl = `${qrInfo.instance_url}${qrInfo.device_pairing_path}`;
			qrDataUrl = await QRCode.toDataURL(fullUrl, {
				width: 256,
				margin: 2,
				color: {
					dark: '#000000',
					light: '#ffffff'
				}
			});
		} catch (error: any) {
			errorMessage = error.message || 'Failed to load QR code. Please try again.';
			showError = true;
		} finally {
			isLoading = false;
		}
	}

	function handleBack() {
		goto('/device');
	}

	function handleRetry() {
		loadQrInfo();
	}
</script>

<svelte:head>
	<title>QR Pairing - RustShare</title>
</svelte:head>

<div class="bg-base-200 flex min-h-screen items-center justify-center p-4">
	<div class="card w-full max-w-md bg-base-100 shadow-xl">
		<div class="card-body items-center text-center">
			<h2 class="card-title text-2xl mb-2">RustShare</h2>
			<p class="text-base-content/70 mb-6">Scan QR code to pair device</p>

			{#if isLoading}
				<div class="flex flex-col items-center py-8">
					<span class="loading loading-spinner loading-lg text-primary"></span>
					<p class="mt-4 text-sm opacity-70">Loading QR code...</p>
				</div>
			{:else if showError}
				<div class="alert alert-error mb-6">
					<svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
					<span>{errorMessage}</span>
				</div>
				<button class="btn btn-primary w-full" on:click={handleRetry}>
					Try Again
				</button>
			{:else if qrDataUrl}
				<div class="w-full space-y-6">
					<div class="flex justify-center">
						<div class="bg-white p-4 rounded-lg shadow-inner">
							<img src={qrDataUrl} alt="Device pairing QR code" class="w-64 h-64" />
						</div>
					</div>

					<div class="space-y-2 text-sm text-base-content/70 max-w-xs mx-auto">
						<p class="font-medium text-base-content">How to pair your device:</p>
						<ol class="text-left space-y-1 list-decimal list-inside">
							<li>Open RustShare on your mobile device</li>
							<li>Go to Settings &gt; Devices</li>
							<li>Tap "Scan QR Code" or enter the URL manually</li>
						</ol>
					</div>

					{#if qrInfo}
						<div class="text-xs text-base-content/50 pt-2">
							<p>URL: {qrInfo.instance_url}{qrInfo.device_pairing_path}</p>
						</div>
					{/if}

					<div class="card-actions justify-center pt-4">
						<button class="btn btn-ghost btn-sm" on:click={handleBack}>
							<svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 19l-7-7m0 0l7-7m-7 7h18" />
							</svg>
							Back to pairing
						</button>
					</div>
				</div>
			{/if}
		</div>
	</div>
</div>

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
