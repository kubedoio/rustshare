<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Html5Qrcode } from 'html5-qrcode';

	interface Props {
		onSuccess: (url: string) => void;
		onError: (error: string) => void;
		onClose: () => void;
		inline?: boolean;
	}

	let { onSuccess, onError, onClose, inline = false }: Props = $props();

	let scanner: Html5Qrcode | null = null;
	let isScanning = $state(false);
	let scannerError = $state('');
	let isLoading = $state(true);
	let lastErrorLogTime = 0;
	const ERROR_LOG_THROTTLE_MS = 5000;

	// Use different element IDs for inline vs modal
	const elementId = inline ? 'qr-reader-inline' : 'qr-reader';

	onMount(async () => {
		try {
			scanner = new Html5Qrcode(elementId);
			await startScanning();
		} catch (error: any) {
			scannerError = error?.message || 'Failed to initialize QR scanner';
			isLoading = false;
		}
	});

	onDestroy(() => {
		stopScanning();
	});

	async function startScanning() {
		if (!scanner) return;

		try {
			isLoading = true;
			scannerError = '';

			// Get available cameras first
			const devices = await Html5Qrcode.getCameras();

			if (!devices || devices.length === 0) {
				throw new Error('No cameras found on this device');
			}

			// Prefer back camera on mobile, fallback to any available camera
			const backCamera = devices.find(d => d.label.toLowerCase().includes('back'));
			const selectedCamera = backCamera || devices[0];

			await scanner.start(
				selectedCamera.id,
				{
					fps: 10,
					qrbox: { width: 250, height: 250 },
					aspectRatio: 1.0
				},
				(decodedText) => {
					// QR code detected
					if (decodedText) {
						// Validate it's a URL
						try {
							new URL(decodedText);
							onSuccess(decodedText);
						} catch {
							// Not a valid URL, but might be a relative path
							if (decodedText.startsWith('/')) {
								onSuccess(decodedText);
							} else {
								scannerError = 'Scanned code is not a valid URL';
							}
						}
					}
				},
				(errorMessage) => {
					// Scan error (no QR code in frame) - ignore these, they're expected
					if (!errorMessage?.includes('NotFoundException')) {
						const now = Date.now();
						if (now - lastErrorLogTime > ERROR_LOG_THROTTLE_MS) {
							console.warn('QR scan error:', errorMessage);
							lastErrorLogTime = now;
						}
					}
				}
			);

			isScanning = true;
			isLoading = false;
		} catch (error: any) {
			isLoading = false;
			if (error?.message?.includes('Permission denied') || error?.name === 'NotAllowedError') {
				scannerError = 'Camera permission denied. Please allow camera access and try again.';
			} else if (error?.message?.includes('NotFoundError')) {
				scannerError = 'Camera not found. Please ensure your device has a camera.';
			} else {
				scannerError = error?.message || 'Failed to start camera scanner';
			}
		}
	}

	async function stopScanning() {
		if (scanner && isScanning) {
			try {
				await scanner.stop();
			} catch (error) {
				console.warn('Error stopping scanner:', error);
			}
			isScanning = false;
		}
	}

	function handleClose() {
		stopScanning();
		onClose();
	}

	function handleRetry() {
		startScanning();
	}
</script>

{#if inline}
	<!-- Inline Mode -->
	<div class="w-full">
		{#if isLoading}
			<div class="flex flex-col items-center justify-center py-8">
				<span class="loading loading-spinner loading-lg text-primary"></span>
				<p class="mt-4 text-sm opacity-70">Starting camera...</p>
			</div>
		{:else if scannerError}
			<div class="alert alert-error mb-4 text-sm">
				<svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-5 w-5" fill="none" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
				</svg>
				<span>{scannerError}</span>
			</div>
			<div class="flex gap-2 justify-center">
				<button class="btn btn-primary btn-sm" onclick={handleRetry}>Try Again</button>
				<button class="btn btn-ghost btn-sm" onclick={handleClose}>Back</button>
			</div>
		{:else}
			<div class="relative">
				<div id="qr-reader-inline" class="w-full aspect-square rounded-lg overflow-hidden bg-black"></div>
				<div class="absolute inset-0 pointer-events-none">
					<div class="absolute top-3 left-3 w-6 h-6 border-l-2 border-t-2 border-primary rounded-tl-lg"></div>
					<div class="absolute top-3 right-3 w-6 h-6 border-r-2 border-t-2 border-primary rounded-tr-lg"></div>
					<div class="absolute bottom-3 left-3 w-6 h-6 border-l-2 border-b-2 border-primary rounded-bl-lg"></div>
					<div class="absolute bottom-3 right-3 w-6 h-6 border-r-2 border-b-2 border-primary rounded-br-lg"></div>
					<div class="absolute left-1/4 right-1/4 top-1/2 h-0.5 bg-primary/50 animate-pulse"></div>
				</div>
			</div>
			<div class="flex justify-center mt-4">
				<button class="btn btn-ghost btn-sm" onclick={handleClose}>Cancel Scan</button>
			</div>
		{/if}
	</div>
{:else}
	<!-- Modal Mode -->
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4">
		<div class="bg-base-100 w-full max-w-md rounded-lg shadow-2xl overflow-hidden">
			<!-- Header -->
			<div class="bg-primary text-primary-content p-4 flex justify-between items-center">
				<h3 class="font-bold text-lg flex items-center gap-2">
					<svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z" />
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 13a3 3 0 11-6 0 3 3 0 016 0z" />
					</svg>
					Scan QR Code
				</h3>
				<button class="btn btn-ghost btn-sm btn-circle" onclick={handleClose} aria-label="Close">
					<svg xmlns="http://www.w3.org/2000/svg" class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
					</svg>
				</button>
			</div>

			<!-- Scanner container -->
			<div class="p-4">
				{#if isLoading}
					<div class="flex flex-col items-center justify-center py-12">
						<span class="loading loading-spinner loading-lg text-primary"></span>
						<p class="mt-4 text-sm opacity-70">Starting camera...</p>
					</div>
				{:else if scannerError}
					<div class="alert alert-error mb-4">
						<svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
						</svg>
						<span>{scannerError}</span>
					</div>
					<div class="flex gap-2 justify-center">
						<button class="btn btn-primary" onclick={handleRetry}>
							<svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
							</svg>
							Try Again
						</button>
						<button class="btn btn-ghost" onclick={handleClose}>
							Cancel
						</button>
					</div>
				{:else}
					<div class="relative">
						<!-- Scanner viewport -->
						<div id="qr-reader" class="w-full aspect-square rounded-lg overflow-hidden bg-black">
							<!-- Html5Qrcode will inject the video element here -->
						</div>

						<!-- Scanning overlay frame -->
						<div class="absolute inset-0 pointer-events-none">
							<!-- Corner markers -->
							<div class="absolute top-4 left-4 w-8 h-8 border-l-4 border-t-4 border-primary rounded-tl-lg"></div>
							<div class="absolute top-4 right-4 w-8 h-8 border-r-4 border-t-4 border-primary rounded-tr-lg"></div>
							<div class="absolute bottom-4 left-4 w-8 h-8 border-l-4 border-b-4 border-primary rounded-bl-lg"></div>
							<div class="absolute bottom-4 right-4 w-8 h-8 border-r-4 border-b-4 border-primary rounded-br-lg"></div>

							<!-- Center scanning line animation -->
							<div class="absolute left-1/4 right-1/4 top-1/2 h-0.5 bg-primary/50 shadow-[0_0_10px_rgba(var(--color-primary),0.8)] animate-pulse"></div>
						</div>
					</div>

					<p class="text-center text-sm text-base-content/70 mt-4">
						Point your camera at a QR code to scan
					</p>

					<div class="flex justify-center mt-4">
						<button class="btn btn-ghost btn-sm" onclick={handleClose}>
							Cancel
						</button>
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	/* Ensure the scanner video element fills the container */
	:global(#qr-reader video, #qr-reader-inline video) {
		width: 100% !important;
		height: 100% !important;
		object-fit: cover !important;
	}

	/* Hide the default html5-qrcode UI elements we don't need */
	:global(#qr-reader__dashboard, #qr-reader-inline__dashboard) {
		display: none !important;
	}

	:global(#qr-reader__scan_region, #qr-reader-inline__scan_region) {
		background: transparent !important;
	}

	:global(#qr-reader__scan_region img, #qr-reader-inline__scan_region img) {
		display: none !important;
	}
</style>
