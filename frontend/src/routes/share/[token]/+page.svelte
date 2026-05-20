<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
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
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import FileIcon from '$lib/components/icons/FileIcon.svelte';

	function generateUUID(): string {
		if (typeof crypto !== 'undefined' && crypto.randomUUID) {
			return crypto.randomUUID();
		}
		return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
			const r = (Math.random() * 16) | 0;
			const v = c === 'x' ? r : (r & 0x3) | 0x8;
			return v.toString(16);
		});
	}

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

	let sessionToken = $state('');
	let password = $state('');
	let passwordError = $state('');
	let isSubmittingPassword = $state(false);
	let isDownloading = $state(false);
	let isUploading = $state(false);
	let errorType: 'not-found' | 'expired' | 'general' | null = $state(null);
	let hasTriedAutoSession = $state(false);
	let uploadInput: HTMLInputElement | null = $state(null);
	let isDragActive = $state(false);
	let uploadQueue: UploadQueueItem[] = $state([]);
	let uploaderName = $state('');

	let currentFolderId = $derived($page.url.searchParams.get('folder'));

	let shareQuery = $derived(createQuery({
		queryKey: ['public-share', token],
		queryFn: () => getPublicShareInfo(token),
		enabled: Boolean(token)
	}));

	let folderContentsQuery = $derived(createQuery({
		queryKey: ['public-share-folder', token, currentFolderId, sessionToken],
		queryFn: () => getPublicFolderContents(token, sessionToken, currentFolderId || undefined),
		enabled: Boolean(
			token &&
				sessionToken &&
				$shareQuery.data?.resource_type === 'folder' &&
				!$shareQuery.data?.upload_only
		)
	}));

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

	$effect(() => {
		if (typeof sessionStorage !== 'undefined') {
			sessionStorage.setItem(UPLOADER_NAME_STORAGE_KEY, uploaderName);
		}
	});

	$effect(() => {
		if (
			$shareQuery.data &&
			!$shareQuery.data.password_protected &&
			!sessionToken &&
			!isSubmittingPassword &&
			!hasTriedAutoSession
		) {
			createSessionAutomatically();
		}
	});

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

	let needsPassword = $derived($shareQuery.data?.password_protected && !sessionToken);
	let canAccessShare = $derived(Boolean($shareQuery.data && sessionToken));
	let canUploadToFolder = $derived(Boolean(
		$shareQuery.data &&
		$shareQuery.data.resource_type === 'folder' &&
		($shareQuery.data.upload_only || $shareQuery.data.permissions !== 'View') &&
		sessionToken
	));

	$effect(() => {
		if ($shareQuery.error) {
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
	});

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
			console.error('Failed to download file:', error);
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
			console.error('Failed to download file:', error);
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
			id: `${file.name}-${file.size}-${generateUUID()}`,
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

<div class="flex min-h-screen items-center justify-center bg-base-200 p-4">
	<div class="card w-full max-w-4xl bg-base-100 shadow-xl">
		<div class="card-body">
			{#if $shareQuery.isLoading}
				<div class="flex flex-col items-center justify-center py-8">
					<span class="loading loading-lg loading-spinner"></span>
					<p class="mt-4 text-base-content/70">Loading share information...</p>
				</div>
			{:else if $shareQuery.isError}
				<div class="flex flex-col items-center justify-center py-8">
					{#if errorType === 'expired'}
						<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-error/10">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="1.5"
								stroke-linecap="round"
								stroke-linejoin="round"
								class="h-8 w-8 text-error"
							>
								<circle cx="12" cy="12" r="10" />
								<polyline points="12 6 12 12 16 14" />
							</svg>
						</div>
						<h2 class="mb-2 card-title text-error">Share Expired</h2>
						<p class="text-center text-base-content/70">
							This share link has expired and is no longer available.
						</p>
					{:else if errorType === 'not-found'}
						<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-error/10">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="1.5"
								stroke-linecap="round"
								stroke-linejoin="round"
								class="h-8 w-8 text-error"
							>
								<circle cx="11" cy="11" r="8" />
								<line x1="21" x2="16.65" y1="21" y2="16.65" />
								<line x1="8" x2="14" y1="11" y2="11" />
							</svg>
						</div>
						<h2 class="mb-2 card-title text-error">Share Not Found</h2>
						<p class="text-center text-base-content/70">
							This share link is invalid or has been revoked.
						</p>
					{:else}
						<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-error/10">
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="1.5"
								stroke-linecap="round"
								stroke-linejoin="round"
								class="h-8 w-8 text-error"
							>
								<path
									d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"
								/>
								<line x1="12" x2="12" y1="9" y2="13" />
								<line x1="12" x2="12.01" y1="17" y2="17" />
							</svg>
						</div>
						<h2 class="mb-2 card-title text-error">Error Loading Share</h2>
						<p class="text-center text-base-content/70">
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
					<div class="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-brand-500/10">
						{#if shareInfo.resource_type === 'folder'}
							<svg
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="1.5"
								stroke-linecap="round"
								stroke-linejoin="round"
								class="h-8 w-8 text-brand-500"
							>
								<path
									d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
								/>
							</svg>
						{:else}
							<FileIcon
								mimeType={shareInfo.mime_type || 'application/octet-stream'}
								size="lg"
								iconClass="text-brand-500"
							/>
						{/if}
					</div>

					<h2 class="mb-2 card-title text-center break-all">{shareInfo.name}</h2>

					<p class="mb-4 text-base-content/70">
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
								class="h-6 w-6 shrink-0 stroke-current"
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
							<p class="font-semibold text-error">This share has expired</p>
							<p class="mt-2 text-sm text-base-content/70">
								The shared resource is no longer available.
							</p>
						</div>
					{:else if needsPassword}
						<form on:submit={handlePasswordSubmit} class="w-full max-w-md">
							<div class="form-control w-full">
								<label for="password" class="label">
									<span class="label-text">This share is password protected</span>
								</label>
								<input
									type="password"
									id="password"
									placeholder="Enter password"
									class="input-bordered input w-full"
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
								class="btn mt-4 w-full btn-primary"
								disabled={isSubmittingPassword || !password}
							>
								{#if isSubmittingPassword}
									<span class="loading loading-sm loading-spinner"></span>
									Verifying...
								{:else}
									Unlock Share
								{/if}
							</button>
						</form>
					{:else if shareInfo.resource_type === 'file' && canAccessShare}
						<button
							type="button"
							class="btn w-full max-w-md btn-lg btn-primary"
							on:click={handleFileDownload}
							disabled={isDownloading}
						>
							{#if isDownloading}
								<span class="loading loading-sm loading-spinner"></span>
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

									<div class="flex items-center justify-between gap-3">
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
													class="btn btn-sm btn-primary"
													on:click={promptFolderUpload}
													disabled={isUploading}
												>
													{#if isUploading}
														<span class="loading loading-xs loading-spinner"></span>
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
												class="input-bordered input w-full"
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
											class={`rounded-lg border-2 border-dashed p-4 text-center transition-colors ${
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
										<div class="space-y-3 rounded-lg border border-base-300 p-4">
											<div class="font-medium">Upload Queue</div>
											{#each uploadQueue as item}
												<div class="space-y-1">
													<div class="flex items-center justify-between gap-3">
														<div class="truncate text-sm">{item.name}</div>
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
								<div class="flex flex-col items-center justify-center py-8">
									<span class="loading loading-lg loading-spinner"></span>
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
								{@const visibleFolders = filterUserVisibleEntries($folderContentsQuery.data.folders ?? [])}
								{@const visibleFiles = filterUserVisibleEntries($folderContentsQuery.data.files ?? [])}
								<div class="space-y-4">
									<div class="flex items-center justify-between gap-3">
										<div>
											<div class="text-sm text-base-content/60">
												{$folderContentsQuery.data.path}
											</div>
											<div class="font-medium">
												{$folderContentsQuery.data.current_folder_name}
											</div>
										</div>
										{#if currentFolderId}
											<button type="button" class="btn btn-ghost btn-sm" on:click={openRootFolder}>
												Back to shared root
											</button>
										{/if}
									</div>

									<div class="flex items-center justify-between gap-3">
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
													class="btn btn-sm btn-primary"
													on:click={promptFolderUpload}
													disabled={isUploading}
												>
													{#if isUploading}
														<span class="loading loading-xs loading-spinner"></span>
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
											class={`rounded-lg border-2 border-dashed p-4 text-center transition-colors ${
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
										<div class="space-y-3 rounded-lg border border-base-300 p-4">
											<div class="font-medium">Upload Queue</div>
											{#each uploadQueue as item}
												<div class="space-y-1">
													<div class="flex items-center justify-between gap-3">
														<div class="truncate text-sm">{item.name}</div>
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

									<div class="overflow-x-auto rounded-lg border border-base-300">
										<table class="table">
											<thead>
												<tr>
													<th>Name</th>
													<th>Type</th>
													<th class="text-right">Action</th>
												</tr>
											</thead>
											<tbody>
												{#each visibleFolders as folder}
													<tr>
														<td>
															<button
																type="button"
																class="btn px-0 normal-case btn-ghost btn-sm"
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
												{#each visibleFiles as file}
													<tr>
														<td>📄 {file.name}</td>
														<td>{formatFileSize(file.size)}</td>
														<td class="text-right">
															<button
																type="button"
																class="btn btn-sm btn-primary"
																on:click={() => handleFolderFileDownload(file)}
																disabled={isDownloading}
															>
																Download
															</button>
														</td>
													</tr>
												{/each}
												{#if visibleFolders.length === 0 && visibleFiles.length === 0}
													<tr>
														<td colspan="3" class="py-8 text-center text-base-content/60">
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
