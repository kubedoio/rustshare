<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '@tanstack/svelte-query';
	import { downloadFile, getFile } from '$lib/api/files';
	import { getFolderContents } from '$lib/api/folders';
	import { listReceivedShares } from '$lib/api/shares';
	import type { File, Folder, ReceivedShare } from '$lib/api/types';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import { formatDate, formatFileSize, getMimeTypeIcon } from '$lib/utils/format';

	type SharedResourceType = 'file' | 'folder';

	let showPreviewModal = false;
	let previewFile: File | null = null;
	let currentFolderId: string | null = null;
	let activeSharedFolderId: string | null = null;
	let nestedPath: Folder[] = [];

	$: resourceId = $page.params.resourceId ?? '';
	$: resourceType = (($page.params.resourceType as SharedResourceType | undefined) ?? 'file');

	const receivedSharesQuery = createQuery({
		queryKey: ['received-shares'],
		queryFn: listReceivedShares
	});

	$: shareEntry = findShare($receivedSharesQuery.data, resourceType, resourceId);
	$: rootFolderId = resourceType === 'folder' ? resourceId : null;

	$: if (resourceType === 'folder' && rootFolderId && activeSharedFolderId !== rootFolderId) {
		activeSharedFolderId = rootFolderId;
		currentFolderId = rootFolderId;
		nestedPath = [];
	}

	$: if (resourceType !== 'folder') {
		activeSharedFolderId = null;
		currentFolderId = null;
		nestedPath = [];
	}

	$: fileQuery = createQuery({
		queryKey: ['shared-file', resourceId],
		queryFn: () => getFile(resourceId),
		enabled: resourceType === 'file' && !!shareEntry
	});

	$: folderContentsQuery = createQuery({
		queryKey: ['shared-folder-contents', currentFolderId],
		queryFn: () => getFolderContents(currentFolderId),
		enabled: resourceType === 'folder' && !!shareEntry && !!currentFolderId
	});

	function findShare(
		shares: ReceivedShare[] | undefined,
		type: SharedResourceType,
		id: string
	): ReceivedShare | undefined {
		return shares?.find((share) => share.resource_type === type && share.resource_id === id);
	}

	function permissionLabel(permission: 'View' | 'Edit' | 'Admin'): string {
		if (permission === 'Admin') return 'Manage';
		if (permission === 'Edit') return 'Edit';
		return 'View';
	}

	function navigateToRootFolder() {
		if (!rootFolderId) return;
		currentFolderId = rootFolderId;
		nestedPath = [];
	}

	function navigateToNestedFolder(index: number) {
		const target = nestedPath[index];
		if (!target) return;
		currentFolderId = target.id;
		nestedPath = nestedPath.slice(0, index + 1);
	}

	function openNestedFolder(folder: Folder) {
		currentFolderId = folder.id;
		nestedPath = [...nestedPath, folder];
	}

	function openPreview(file: File) {
		previewFile = file;
		showPreviewModal = true;
	}

	async function handleDownload(file: File) {
		try {
			const response = await downloadFile(file.id);
			let downloadUrl = response.url;

			if (downloadUrl.includes('/rustshare-files/')) {
				const path = downloadUrl.split('/rustshare-files/')[1];
				downloadUrl = `/storage/${path}`;
			}

			window.open(downloadUrl, '_blank');
		} catch (error) {
			console.error('Failed to download shared file', error);
		}
	}

	$: currentFolderTitle =
		nestedPath.length > 0 ? nestedPath[nestedPath.length - 1].name : shareEntry?.resource_name;
</script>

<svelte:head>
	<title>
		{shareEntry ? `${shareEntry.resource_name} - Shared with Me - RustShare` : 'Shared Resource - RustShare'}
	</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center justify-between gap-4">
		<div>
			<button class="btn btn-ghost btn-sm -ml-2 mb-2" on:click={() => goto('/shared-with-me')}>
				← Back to Shared with Me
			</button>
			<h1 class="text-3xl font-bold">
				{#if shareEntry}
					{shareEntry.resource_name}
				{:else}
					Shared Resource
				{/if}
			</h1>
			{#if shareEntry}
				<p class="mt-1 text-base-content/70">
					Shared by {shareEntry.shared_by_name} • {permissionLabel(shareEntry.permission)} access
				</p>
			{/if}
		</div>
	</div>

	{#if $receivedSharesQuery.isLoading}
		<div class="py-12 flex justify-center">
			<span class="loading loading-spinner loading-lg"></span>
		</div>
	{:else if $receivedSharesQuery.isError}
		<div class="alert alert-error">
			<span>Failed to load shared resources: {$receivedSharesQuery.error?.message}</span>
		</div>
	{:else if !shareEntry}
		<div class="alert alert-warning">
			<span>This shared resource is no longer available or you no longer have access to it.</span>
		</div>
	{:else if resourceType === 'file'}
		<div class="grid gap-6 lg:grid-cols-[minmax(0,2fr)_20rem]">
			<div class="card bg-base-100 shadow-xl">
				<div class="card-body">
					<h2 class="card-title">Shared File</h2>
					{#if $fileQuery.isLoading}
						<div class="py-12 flex justify-center">
							<span class="loading loading-spinner loading-lg"></span>
						</div>
					{:else if $fileQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load file: {$fileQuery.error?.message}</span>
						</div>
					{:else if $fileQuery.data}
						<div class="space-y-5">
							<div class="flex items-start gap-4">
								<div class="text-5xl">{getMimeTypeIcon($fileQuery.data.mime_type)}</div>
								<div>
									<div class="text-xl font-semibold">{$fileQuery.data.name}</div>
									<div class="text-sm text-base-content/60">{$fileQuery.data.path}</div>
								</div>
							</div>

							<div class="grid gap-4 sm:grid-cols-2">
								<div class="rounded-lg bg-base-200 p-4">
									<div class="text-sm text-base-content/60">Type</div>
									<div class="font-medium">{$fileQuery.data.mime_type}</div>
								</div>
								<div class="rounded-lg bg-base-200 p-4">
									<div class="text-sm text-base-content/60">Size</div>
									<div class="font-medium">{formatFileSize($fileQuery.data.size)}</div>
								</div>
								<div class="rounded-lg bg-base-200 p-4">
									<div class="text-sm text-base-content/60">Modified</div>
									<div class="font-medium">{formatDate($fileQuery.data.modified_at)}</div>
								</div>
								<div class="rounded-lg bg-base-200 p-4">
									<div class="text-sm text-base-content/60">Shared</div>
									<div class="font-medium">{formatDate(shareEntry.created_at)}</div>
								</div>
							</div>

							<div class="flex gap-3">
								<button class="btn btn-primary" on:click={() => openPreview($fileQuery.data)}>
									Preview
								</button>
								<button class="btn btn-outline" on:click={() => handleDownload($fileQuery.data)}>
									Download
								</button>
							</div>
						</div>
					{/if}
				</div>
			</div>

			<div class="card bg-base-100 shadow-xl">
				<div class="card-body">
					<h2 class="card-title">Share Details</h2>
					<div class="space-y-4 text-sm">
						<div>
							<div class="text-base-content/60">Shared by</div>
							<div class="font-medium">{shareEntry.shared_by_name}</div>
							{#if shareEntry.shared_by_email}
								<div class="text-base-content/60">{shareEntry.shared_by_email}</div>
							{/if}
						</div>
						<div>
							<div class="text-base-content/60">Permission</div>
							<div class="font-medium">{permissionLabel(shareEntry.permission)}</div>
						</div>
						<div>
							<div class="text-base-content/60">Original path</div>
							<div class="break-all">{shareEntry.resource_path}</div>
						</div>
					</div>
				</div>
			</div>
		</div>
	{:else}
		<div class="space-y-6">
			<div class="card bg-base-100 shadow-xl">
				<div class="card-body">
					<div class="flex items-center justify-between gap-4 flex-wrap">
						<div>
							<h2 class="card-title">Shared Folder</h2>
							<p class="text-sm text-base-content/70">
								Browse the contents of this shared folder without leaving the shared workspace
							</p>
						</div>
						<div class="badge badge-ghost">{permissionLabel(shareEntry.permission)}</div>
					</div>

					<div class="text-sm text-base-content/70 mt-4">
						<div>Shared by {shareEntry.shared_by_name}</div>
						<div>Original path: {shareEntry.resource_path}</div>
					</div>
				</div>
			</div>

			<div class="card bg-base-100 shadow-xl">
				<div class="card-body">
					<div class="flex items-center justify-between gap-4 flex-wrap">
						<div>
							<h2 class="card-title">{currentFolderTitle}</h2>
							<div class="breadcrumbs text-sm">
								<ul>
									<li>
										<button type="button" class="link link-hover" on:click={navigateToRootFolder}>
											{shareEntry.resource_name}
										</button>
									</li>
									{#each nestedPath as folder, index}
										<li>
											<button
												type="button"
												class="link link-hover"
												on:click={() => navigateToNestedFolder(index)}
											>
												{folder.name}
											</button>
										</li>
									{/each}
								</ul>
							</div>
						</div>
					</div>

					{#if $folderContentsQuery.isLoading}
						<div class="py-12 flex justify-center">
							<span class="loading loading-spinner loading-lg"></span>
						</div>
					{:else if $folderContentsQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load folder contents: {$folderContentsQuery.error?.message}</span>
						</div>
					{:else if $folderContentsQuery.data}
						<div class="overflow-x-auto">
							<table class="table table-zebra">
								<thead>
									<tr>
										<th>Name</th>
										<th>Type</th>
										<th>Size</th>
										<th>Modified</th>
										<th class="text-right">Actions</th>
									</tr>
								</thead>
								<tbody>
									{#each $folderContentsQuery.data.folders as folder}
										<tr class="hover">
											<td>
												<button
													type="button"
													class="font-medium flex items-center gap-3 hover:text-primary"
													on:click={() => openNestedFolder(folder)}
												>
													<span class="text-xl">📁</span>
													<span>{folder.name}</span>
												</button>
											</td>
											<td>Folder</td>
											<td>—</td>
											<td>{formatDate(folder.updated_at)}</td>
											<td class="text-right">
												<button
													type="button"
													class="btn btn-sm btn-outline"
													on:click={() => openNestedFolder(folder)}
												>
													Open
												</button>
											</td>
										</tr>
									{/each}

									{#each $folderContentsQuery.data.files as file}
										<tr class="hover">
											<td>
												<button
													type="button"
													class="font-medium flex items-center gap-3 hover:text-primary"
													on:click={() => openPreview(file)}
												>
													<span class="text-xl">{getMimeTypeIcon(file.mime_type)}</span>
													<span>{file.name}</span>
												</button>
											</td>
											<td>{file.mime_type}</td>
											<td>{formatFileSize(file.size)}</td>
											<td>{formatDate(file.modified_at)}</td>
											<td class="text-right">
												<div class="flex justify-end gap-2">
													<button
														type="button"
														class="btn btn-sm btn-ghost"
														on:click={() => openPreview(file)}
													>
														Preview
													</button>
													<button
														type="button"
														class="btn btn-sm btn-outline"
														on:click={() => handleDownload(file)}
													>
														Download
													</button>
												</div>
											</td>
										</tr>
									{/each}

									{#if $folderContentsQuery.data.folders.length === 0 && $folderContentsQuery.data.files.length === 0}
										<tr>
											<td colspan="5" class="py-10 text-center text-base-content/60">
												This folder is empty.
											</td>
										</tr>
									{/if}
								</tbody>
							</table>
						</div>
					{/if}
				</div>
			</div>
		</div>
	{/if}
</div>

<FilePreviewModal
	open={showPreviewModal}
	file={previewFile}
	on:close={() => {
		showPreviewModal = false;
		previewFile = null;
	}}
/>
