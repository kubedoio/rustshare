<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '@tanstack/svelte-query';
	import { onMount } from 'svelte';
	import {
		getPublicShareInfo,
		createShareSession,
		downloadPublicShareFile,
		triggerFileDownload,
		type ShareInfo
	} from '$lib/api/shares';
	import { formatFileSize, getMimeTypeIcon } from '$lib/utils/format';

	const token = $page.params.token ?? '';
	const SESSION_STORAGE_KEY = `share_session_${token}`;

	// Query for share info
	$: shareQuery = createQuery({
		queryKey: ['public-share', token],
		queryFn: () => getPublicShareInfo(token),
		enabled: Boolean(token)
	});

	let sessionToken = '';
	let password = '';
	let passwordError = '';
	let isSubmittingPassword = false;
	let isDownloading = false;
	let errorType: 'not-found' | 'expired' | 'general' | null = null;
	let hasTriedAutoSession = false;

	// Load session token from localStorage on mount
	onMount(() => {
		const stored = localStorage.getItem(SESSION_STORAGE_KEY);
		if (stored) {
			sessionToken = stored;
		}
	});

	// Auto-create session for non-password-protected shares
	$: if (
		$shareQuery.data &&
		!$shareQuery.data.password_protected &&
		!sessionToken &&
		!isSubmittingPassword &&
		!hasTriedAutoSession
	) {
		createSessionAutomatically();
	}

	async function createSessionAutomatically() {
		hasTriedAutoSession = true;
		isSubmittingPassword = true;
		try {
			const response = await createShareSession(token, {});
			sessionToken = response.session_token;
			localStorage.setItem(SESSION_STORAGE_KEY, response.session_token);
		} catch (error) {
			console.error('Failed to create session:', error);
		} finally {
			isSubmittingPassword = false;
		}
	}

	// Check if password is required
	$: needsPassword = $shareQuery.data?.password_protected && !sessionToken;
	$: canDownload = $shareQuery.data && sessionToken;

	// Parse error type from query error
	$: if ($shareQuery.error) {
		const error = $shareQuery.error as any;
		const status = error?.status;
		const message = error?.message?.toLowerCase() || '';

		if (status === 410 || message.includes('expired')) {
			errorType = 'expired';
		} else if (status === 404 || message.includes('not found')) {
			errorType = 'not-found';
		} else {
			errorType = 'general';
		}
	}

	// Check if share is expired (client-side check)
	function isExpired(shareInfo: ShareInfo | undefined): boolean {
		if (!shareInfo?.expires_at) return false;
		return new Date(shareInfo.expires_at) < new Date();
	}

	async function handlePasswordSubmit(e: Event) {
		e.preventDefault();
		passwordError = '';
		isSubmittingPassword = true;

		try {
			const response = await createShareSession(token, { password });
			sessionToken = response.session_token;
			localStorage.setItem(SESSION_STORAGE_KEY, response.session_token);
			password = ''; // Clear password input
		} catch (error) {
			passwordError = error instanceof Error ? error.message : 'Invalid password';
		} finally {
			isSubmittingPassword = false;
		}
	}

	async function handleDownload() {
		if (!$shareQuery.data || !sessionToken) return;

		isDownloading = true;
		try {
			const blob = await downloadPublicShareFile(token, sessionToken);
			triggerFileDownload(blob, $shareQuery.data.file_name);
		} catch (error) {
			alert(error instanceof Error ? error.message : 'Failed to download file');
		} finally {
			isDownloading = false;
		}
	}

	function formatExpiryDate(dateString: string): string {
		const date = new Date(dateString);
		return date.toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'long',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

<svelte:head>
	<title>Shared File - RustShare</title>
</svelte:head>

<div class="bg-base-200 p-4 flex min-h-screen items-center justify-center">
	<div class="card max-w-md bg-base-100 shadow-xl w-full">
		<div class="card-body">
			{#if $shareQuery.isLoading}
				<div class="py-8 flex flex-col items-center justify-center">
					<span class="loading loading-spinner loading-lg"></span>
					<p class="mt-4 text-base-content/70">Loading share information...</p>
				</div>
			{:else if $shareQuery.isError}
				<div class="py-8 flex flex-col items-center justify-center">
					{#if errorType === 'expired'}
						<div class="text-6xl mb-4">⏰</div>
						<h2 class="card-title text-error mb-2">Share Expired</h2>
						<p class="text-base-content/70 text-center">
							This share link has expired and is no longer available.
						</p>
					{:else if errorType === 'not-found'}
						<div class="text-6xl mb-4">🔍</div>
						<h2 class="card-title text-error mb-2">Share Not Found</h2>
						<p class="text-base-content/70 text-center">
							This share link is invalid or has been revoked.
						</p>
					{:else}
						<div class="text-6xl mb-4">⚠️</div>
						<h2 class="card-title text-error mb-2">Error Loading Share</h2>
						<p class="text-base-content/70 text-center">
							{$shareQuery.error instanceof Error
								? $shareQuery.error.message
								: 'Failed to load share information. Please try again later.'}
						</p>
					{/if}
				</div>
			{:else if $shareQuery.data}
				{@const shareInfo = $shareQuery.data}
				{@const expired = isExpired(shareInfo)}

				<div class="flex flex-col items-center">
					<!-- File Icon -->
					<div class="text-6xl mb-4">
						{getMimeTypeIcon(shareInfo.mime_type)}
					</div>

					<!-- File Name -->
					<h2 class="card-title mb-2 text-center break-all">
						{shareInfo.file_name}
					</h2>

					<!-- File Size -->
					<p class="text-base-content/70 mb-4">
						{formatFileSize(shareInfo.file_size)}
					</p>

					<!-- Expiry Warning -->
					{#if shareInfo.expires_at}
						<div class="alert {expired ? 'alert-error' : 'alert-info'} mb-4 w-full">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
								class="w-6 h-6 shrink-0 stroke-current"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
								></path>
							</svg>
							<div class="text-sm">
								{#if expired}
									<span class="font-semibold">Expired</span> on {formatExpiryDate(
										shareInfo.expires_at
									)}
								{:else}
									<span class="font-semibold">Expires</span> on {formatExpiryDate(
										shareInfo.expires_at
									)}
								{/if}
							</div>
						</div>
					{/if}

					{#if expired}
						<!-- Expired State -->
						<div class="py-4 text-center">
							<p class="text-error font-semibold">This share has expired</p>
							<p class="text-base-content/70 text-sm mt-2">
								The file is no longer available for download.
							</p>
						</div>
					{:else if needsPassword}
						<!-- Password Form -->
						<form on:submit={handlePasswordSubmit} class="w-full">
							<div class="form-control w-full">
								<label for="password" class="label">
									<span class="label-text">This file is password protected</span>
								</label>
								<input
									type="password"
									id="password"
									placeholder="Enter password"
									class="input input-bordered w-full"
									bind:value={password}
									disabled={isSubmittingPassword}
									required
								/>
								{#if passwordError}
									<p class="label">
										<span class="label-text-alt text-error">{passwordError}</span>
									</p>
								{/if}
							</div>
							<button
								type="submit"
								class="btn btn-primary mt-4 w-full"
								disabled={isSubmittingPassword || !password}
							>
								{#if isSubmittingPassword}
									<span class="loading loading-spinner loading-sm"></span>
									Verifying...
								{:else}
									Unlock File
								{/if}
							</button>
						</form>
					{:else if canDownload}
						<!-- Download Button -->
						<button
							type="button"
							class="btn btn-primary btn-lg w-full"
							on:click={handleDownload}
							disabled={isDownloading}
						>
							{#if isDownloading}
								<span class="loading loading-spinner loading-sm"></span>
								Downloading...
							{:else}
								<svg
									xmlns="http://www.w3.org/2000/svg"
									class="h-6 w-6"
									fill="none"
									viewBox="0 0 24 24"
									stroke="currentColor"
								>
									<path
										stroke-linecap="round"
										stroke-linejoin="round"
										stroke-width="2"
										d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
									/>
								</svg>
								Download File
							{/if}
						</button>

						{#if $shareQuery.data.password_protected && sessionToken}
							<p class="text-xs text-base-content/60 mt-4 text-center">
								Password verified. Click to download.
							</p>
						{/if}
					{/if}
				</div>

				<!-- Powered by RustShare -->
				<div class="divider"></div>
				<div class="text-center">
					<p class="text-xs text-base-content/60">
						Powered by <span class="font-semibold">RustShare</span>
					</p>
				</div>
			{/if}
		</div>
	</div>
</div>
