<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { createQuery } from '@tanstack/svelte-query';
	import { onMount } from 'svelte';
	import type { File as SharedFile } from '$lib/api/types';
	import {
		createShareSession,
		downloadPublicFolderFile,
		downloadPublicShareFile,
		getPublicFolderContents,
		getPublicShareInfo,
		uploadToPublicFolder,
		triggerFileDownload,
		type ShareInfo
	} from '$lib/api/shares';
	import { queryClient } from '$lib/query-client';
	import { toastStore } from '$lib/stores/toast';
	import { formatFileSize, getMimeTypeIcon } from '$lib/utils/format';

	type UploadQueueItem = {
		id: string;
		name: string;
		progress: number;
		status: 'queued' | 'uploading' | 'done' | 'error';
		error?: string;
	};

	const token = $page.params.token ?? '';
	const SESSION_STORAGE_KEY = `share_session_${token}`;
	const UPLOADER_NAME_STORAGE_KEY = `share_uploader_name_${token}`;

	$: currentFolderId = $page.url.searchParams.get('folder');

	const shareQuery = createQuery({
		queryKey: ['public-share', token],
		queryFn: () => getPublicShareInfo(token),
		enabled: Boolean(token)
	});

	const folderContentsQuery = createQuery({
		queryKey: ['public-share-folder', token, currentFolderId, sessionToken],
		queryFn: () => getPublicFolderContents(token, sessionToken, currentFolderId || undefined),
		enabled: Boolean(
			token &&
			sessionToken &&
			$shareQuery.data?.resource_type === 'folder' &&
			!$shareQuery.data?.upload_only
		)
	});

	let sessionToken = '';
	let password = '';
	let passwordError = '';
	let isSubmittingPassword = false;
	let isDownloading = false;
	let isUploading = false;
	let errorType: 'not-found' | 'expired' | 'general' | null = null;
	let hasTriedAutoSession = false;
	let uploadInput: HTMLInputElement | null = null;
	let isDragActive = false;
	let uploadQueue: UploadQueueItem[] = [];
	let uploaderName = '';

	onMount(() => {
		const stored = sessionStorage.getItem(SESSION_STORAGE_KEY);
		if (stored) {
			sessionToken = stored;
		}

		const storedUploaderName = sessionStorage.getItem(UPLOADER_NAME_STORAGE_KEY);
		if (storedUploaderName) {
			uploaderName = storedUploaderName;
		}
	});

	$: if (typeof sessionStorage !== 'undefined') {
		sessionStorage.setItem(UPLOADER_NAME_STORAGE_KEY, uploaderName);
	}

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
			sessionStorage.setItem(SESSION_STORAGE_KEY, response.session_token);
		} catch (error) {
			console.error('Failed to create session:', error);
		} finally {
			isSubmittingPassword = false;
		}
	}

	$: needsPassword = $shareQuery.data?.password_protected && !sessionToken;
	$: canAccessShare = Boolean($shareQuery.data && sessionToken);
	$: canUploadToFolder = Boolean(
		$shareQuery.data &&
		$shareQuery.data.resource_type === 'folder' &&
		($shareQuery.data.upload_only || $shareQuery.data.permissions !== 'View') &&
		sessionToken
	);

	$: if ($shareQuery.error) {
		const error = $shareQuery.error as { status?: number; message?: string };
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

	function isExpired(shareInfo: ShareInfo | undefined): boolean {
		if (!shareInfo?.expires_at) return false;
		return new Date(shareInfo.expires_at) < new Date();
	}

	async function handlePasswordSubmit(event: Event) {
		event.preventDefault();
		passwordError = '';
		isSubmittingPassword = true;

		try {
			const response = await createShareSession(token, { password });
			sessionToken = response.session_token;
			sessionStorage.setItem(SESSION_STORAGE_KEY, response.session_token);
			password = '';
		} catch (error) {
			passwordError = error instanceof Error ? error.message : 'Invalid password';
		} finally {
			isSubmittingPassword = false;
		}
	}

	async function handleFileDownload() {
		if (!$shareQuery.data || !sessionToken) return;

		isDownloading = true;
		try {
			const blob = await downloadPublicShareFile(token, sessionToken);
			triggerFileDownload(blob, $shareQuery.data.name);
		} catch (error) {
			alert(error instanceof Error ? error.message : 'Failed to download file');
		} finally {
			isDownloading = false;
		}
	}

	async function handleFolderFileDownload(file: SharedFile) {
		if (!sessionToken) return;

		isDownloading = true;
		try {
			const blob = await downloadPublicFolderFile(token, file.id, sessionToken);
			triggerFileDownload(blob, file.name);
		} catch (error) {
			alert(error instanceof Error ? error.message : 'Failed to download file');
		} finally {
			isDownloading = false;
		}
	}

	function promptFolderUpload() {
		uploadInput?.click();
	}

	function updateUploadQueue(id: string, patch: Partial<UploadQueueItem>) {
		uploadQueue = uploadQueue.map((item) => (item.id === id ? { ...item, ...patch } : item));
	}

	async function uploadFiles(files: FileList | globalThis.File[]) {
		const fileList = Array.from(files);
		if (fileList.length === 0 || !sessionToken) {
			return;
		}

		isUploading = true;
		const targetFolderId = $shareQuery.data?.upload_only
			? undefined
			: currentFolderId || $folderContentsQuery.data?.root_folder_id;

		const queuedItems = fileList.map((file) => ({
			id: `${file.name}-${file.size}-${crypto.randomUUID()}`,
			name: file.name,
			progress: 0,
			status: 'queued' as const
		}));
		uploadQueue = [...queuedItems, ...uploadQueue];

		for (const [index, file] of fileList.entries()) {
			const itemId = queuedItems[index].id;
			updateUploadQueue(itemId, { status: 'uploading', progress: 0 });

			try {
				await uploadToPublicFolder(token, sessionToken, file, {
					parentFolderId: targetFolderId,
					uploaderName,
					onProgress: (progress) => updateUploadQueue(itemId, { progress })
				});
				updateUploadQueue(itemId, { status: 'done', progress: 100 });
				toastStore.show(`Uploaded "${file.name}"`, 'success');
			} catch (error) {
				updateUploadQueue(itemId, {
					status: 'error',
					error: error instanceof Error ? error.message : 'Failed to upload file'
				});
				toastStore.show(
					error instanceof Error ? error.message : `Failed to upload "${file.name}"`,
					'error'
				);
			}
		}

		await queryClient.invalidateQueries({
			queryKey: ['public-share-folder', token]
		});
		isUploading = false;
	}

	async function handleFolderUpload(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		if (!input.files?.length) {
			return;
		}

		try {
			await uploadFiles(input.files);
		} finally {
			isUploading = false;
			input.value = '';
		}
	}

	async function handleDrop(event: DragEvent) {
		event.preventDefault();
		isDragActive = false;
		if (!event.dataTransfer?.files?.length) {
			return;
		}

		await uploadFiles(event.dataTransfer.files);
	}

	function openFolder(folderId: string) {
		goto(`/share/${token}?folder=${folderId}`);
	}

	function openRootFolder() {
		goto(`/share/${token}`);
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
	<title>Shared Resource - RustShare</title>
</svelte:head>

<div class="bg-base-200 p-4 flex min-h-screen items-center justify-center">
	<div class="card max-w-4xl bg-base-100 shadow-xl w-full">
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
					<div class="text-6xl mb-4">
						{#if shareInfo.resource_type === 'folder'}
							📁
						{:else}
							{getMimeTypeIcon(shareInfo.mime_type || 'application/octet-stream')}
						{/if}
					</div>

					<h2 class="card-title mb-2 text-center break-all">{shareInfo.name}</h2>

					<p class="text-base-content/70 mb-4">
						{#if shareInfo.resource_type === 'folder'}
							{shareInfo.upload_only ? 'Upload-only folder drop' : 'Shared folder'}
						{:else if shareInfo.file_size !== null}
							{formatFileSize(shareInfo.file_size)}
						{/if}
					</p>

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
						<div class="py-4 text-center">
							<p class="text-error font-semibold">This share has expired</p>
							<p class="text-base-content/70 text-sm mt-2">
								The shared resource is no longer available.
							</p>
						</div>
					{:else if needsPassword}
						<form on:submit={handlePasswordSubmit} class="max-w-md w-full">
							<div class="form-control w-full">
								<label for="password" class="label">
									<span class="label-text">This share is password protected</span>
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
									Unlock Share
								{/if}
							</button>
						</form>
					{:else if shareInfo.resource_type === 'file' && canAccessShare}
						<button
							type="button"
							class="btn btn-primary btn-lg max-w-md w-full"
							on:click={handleFileDownload}
							disabled={isDownloading}
						>
							{#if isDownloading}
								<span class="loading loading-spinner loading-sm"></span>
								Downloading...
							{:else}
								Download File
							{/if}
						</button>
					{:else if shareInfo.resource_type === 'folder' && canAccessShare}
						<div class="w-full">
							{#if shareInfo.upload_only}
								<div class="space-y-4">
									<div class="alert alert-info">
										<span>
											This link accepts uploads into <strong>{shareInfo.name}</strong> but does not allow
											browsing or downloading existing files.
										</span>
									</div>

									<div class="gap-3 flex items-center justify-between">
										<div class="text-sm text-base-content/60">
											Upload files directly to the shared folder root.
										</div>
										{#if canUploadToFolder}
											<div>
												<input
													bind:this={uploadInput}
													type="file"
													multiple
													class="hidden"
													on:change={handleFolderUpload}
												/>
												<button
													type="button"
													class="btn btn-primary btn-sm"
													on:click={promptFolderUpload}
													disabled={isUploading}
												>
													{#if isUploading}
														<span class="loading loading-spinner loading-xs"></span>
														Uploading...
													{:else}
														Upload Files
													{/if}
												</button>
											</div>
										{/if}
									</div>

									{#if canUploadToFolder}
										<label class="form-control w-full">
											<div class="label">
												<span class="label-text">Your name (optional)</span>
												<span class="label-text-alt">Shown in upload audit history</span>
											</div>
											<input
												type="text"
												class="input input-bordered w-full"
												bind:value={uploaderName}
												maxlength="120"
												placeholder="Jane from Marketing"
											/>
										</label>
									{/if}

									{#if canUploadToFolder}
										<div
											role="region"
											aria-label="Drag and drop upload area"
											class={`rounded-lg p-4 border-2 border-dashed text-center transition-colors ${
												isDragActive ? 'border-primary bg-primary/5' : 'border-base-300'
											}`}
											on:dragenter|preventDefault={() => (isDragActive = true)}
											on:dragover|preventDefault={() => (isDragActive = true)}
											on:dragleave|preventDefault={() => (isDragActive = false)}
											on:drop={handleDrop}
										>
											<p class="font-medium">Drag files here to upload</p>
											<p class="text-sm text-base-content/60">
												Files will be uploaded into {shareInfo.name}.
											</p>
										</div>
									{/if}

									{#if uploadQueue.length > 0}
										<div class="rounded-lg border-base-300 p-4 space-y-3 border">
											<div class="font-medium">Upload Queue</div>
											{#each uploadQueue as item}
												<div class="space-y-1">
													<div class="gap-3 flex items-center justify-between">
														<div class="text-sm truncate">{item.name}</div>
														<div class="text-xs text-base-content/60">
															{#if item.status === 'done'}
																Done
															{:else if item.status === 'error'}
																Failed
															{:else if item.status === 'uploading'}
																{item.progress}%
															{:else}
																Queued
															{/if}
														</div>
													</div>
													<progress
														class="progress w-full {item.status === 'error'
															? 'progress-error'
															: 'progress-primary'}"
														value={item.status === 'error' ? 100 : item.progress}
														max="100"
													></progress>
													{#if item.error}
														<div class="text-xs text-error">{item.error}</div>
													{/if}
												</div>
											{/each}
										</div>
									{/if}
								</div>
							{:else if $folderContentsQuery.isLoading}
								<div class="py-8 flex flex-col items-center justify-center">
									<span class="loading loading-spinner loading-lg"></span>
									<p class="mt-4 text-base-content/70">Loading shared folder...</p>
								</div>
							{:else if $folderContentsQuery.isError}
								<div class="alert alert-error">
									<span>
										{$folderContentsQuery.error instanceof Error
											? $folderContentsQuery.error.message
											: 'Failed to load folder contents'}
									</span>
								</div>
							{:else if $folderContentsQuery.data}
								<div class="space-y-4">
									<div class="gap-3 flex items-center justify-between">
										<div>
											<div class="text-sm text-base-content/60">
												{$folderContentsQuery.data.path}
											</div>
											<div class="font-medium">
												{$folderContentsQuery.data.current_folder_name}
											</div>
										</div>
										{#if currentFolderId}
											<button type="button" class="btn btn-sm btn-ghost" on:click={openRootFolder}>
												Back to shared root
											</button>
										{/if}
									</div>

									<div class="gap-3 flex items-center justify-between">
										<div class="text-sm text-base-content/60">
											{#if shareInfo.permissions === 'View'}
												This link is view-only.
											{:else}
												This link allows uploads into the current shared folder.
											{/if}
										</div>
										{#if canUploadToFolder}
											<div>
												<input
													bind:this={uploadInput}
													type="file"
													multiple
													class="hidden"
													on:change={handleFolderUpload}
												/>
												<button
													type="button"
													class="btn btn-primary btn-sm"
													on:click={promptFolderUpload}
													disabled={isUploading}
												>
													{#if isUploading}
														<span class="loading loading-spinner loading-xs"></span>
														Uploading...
													{:else}
														Upload File
													{/if}
												</button>
											</div>
										{/if}
									</div>

									{#if canUploadToFolder}
										<div
											role="region"
											aria-label="Drag and drop upload area"
											class={`rounded-lg p-4 border-2 border-dashed text-center transition-colors ${
												isDragActive ? 'border-primary bg-primary/5' : 'border-base-300'
											}`}
											on:dragenter|preventDefault={() => (isDragActive = true)}
											on:dragover|preventDefault={() => (isDragActive = true)}
											on:dragleave|preventDefault={() => (isDragActive = false)}
											on:drop={handleDrop}
										>
											<p class="font-medium">Drag files here to upload</p>
											<p class="text-sm text-base-content/60">
												Files will be uploaded into {$folderContentsQuery.data.current_folder_name}.
											</p>
										</div>
									{/if}

									{#if uploadQueue.length > 0}
										<div class="rounded-lg border-base-300 p-4 space-y-3 border">
											<div class="font-medium">Upload Queue</div>
											{#each uploadQueue as item}
												<div class="space-y-1">
													<div class="gap-3 flex items-center justify-between">
														<div class="text-sm truncate">{item.name}</div>
														<div class="text-xs text-base-content/60">
															{#if item.status === 'done'}
																Done
															{:else if item.status === 'error'}
																Failed
															{:else if item.status === 'uploading'}
																{item.progress}%
															{:else}
																Queued
															{/if}
														</div>
													</div>
													<progress
														class="progress w-full {item.status === 'error'
															? 'progress-error'
															: 'progress-primary'}"
														value={item.status === 'error' ? 100 : item.progress}
														max="100"
													></progress>
													{#if item.error}
														<div class="text-xs text-error">{item.error}</div>
													{/if}
												</div>
											{/each}
										</div>
									{/if}

									<div class="rounded-lg border-base-300 overflow-x-auto border">
										<table class="table">
											<thead>
												<tr>
													<th>Name</th>
													<th>Type</th>
													<th class="text-right">Action</th>
												</tr>
											</thead>
											<tbody>
												{#each $folderContentsQuery.data.folders as folder}
													<tr>
														<td>
															<button
																type="button"
																class="btn btn-ghost btn-sm px-0 normal-case"
																on:click={() => openFolder(folder.id)}
															>
																📁 {folder.name}
															</button>
														</td>
														<td>Folder</td>
														<td class="text-right">
															<button
																type="button"
																class="btn btn-ghost btn-sm"
																on:click={() => openFolder(folder.id)}
															>
																Open
															</button>
														</td>
													</tr>
												{/each}
												{#each $folderContentsQuery.data.files as file}
													<tr>
														<td>📄 {file.name}</td>
														<td>{formatFileSize(file.size)}</td>
														<td class="text-right">
															<button
																type="button"
																class="btn btn-primary btn-sm"
																on:click={() => handleFolderFileDownload(file)}
																disabled={isDownloading}
															>
																Download
															</button>
														</td>
													</tr>
												{/each}
												{#if $folderContentsQuery.data.folders.length === 0 && $folderContentsQuery.data.files.length === 0}
													<tr>
														<td colspan="3" class="text-base-content/60 py-8 text-center">
															This folder is empty.
														</td>
													</tr>
												{/if}
											</tbody>
										</table>
									</div>
								</div>
							{/if}
						</div>
					{/if}
				</div>

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
