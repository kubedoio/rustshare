<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { downloadFile, getFile } from '$lib/api/files';
	import { getFolder, getSharedFolderContents, getSharedFolderTree } from '$lib/api/folders';
	import { findFolderPathInTree } from '$lib/explorer/breadcrumbs';
	import { listReceivedShares } from '$lib/api/shares';
	import type { File, Folder, ReceivedShare } from '$lib/api/types';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { sharedResourcePath } from '$lib/utils/shared';
	import { formatDate, formatFileSize } from '$lib/utils/format';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import FileIcon from '$lib/components/icons/FileIcon.svelte';

	type SharedResourceType = 'file' | 'folder';

	let showPreviewModal = $state(false);
	let previewFile = $state<File | null>(null);

	let resourceId = $derived($page.params.resourceId ?? '');
	let resourceType = $derived(($page.params.resourceType as SharedResourceType | undefined) ?? 'file');
	let requestedFolderId = $derived($page.url.searchParams.get('folder'));

	const receivedSharesQuery = createQuery({
		queryKey: ['received-shares'],
		queryFn: listReceivedShares
	});

	let shareEntry = $derived(findShare($receivedSharesQuery.data, resourceType, resourceId));
	let rootFolderId = $derived(resourceType === 'folder' ? resourceId : null);
	let currentFolderId = $derived(resourceType === 'folder' ? (requestedFolderId || rootFolderId) : null);

	let fileQuery = $derived(createQuery({
		queryKey: ['shared-file', resourceId],
		queryFn: () => getFile(resourceId),
		enabled: resourceType === 'file' && !!shareEntry
	}));

	let folderContentsQuery = $derived(createQuery({
		queryKey: ['shared-folder-contents', currentFolderId],
		queryFn: () => getSharedFolderContents(currentFolderId!),
		enabled: resourceType === 'folder' && !!shareEntry && !!currentFolderId
	}));

	let currentFolderQuery = $derived(createQuery({
		queryKey: ['shared-folder-meta', currentFolderId],
		queryFn: () => getFolder(currentFolderId as string),
		enabled:
			resourceType === 'folder' &&
			!!shareEntry &&
			!!currentFolderId &&
			currentFolderId !== rootFolderId
	}));

	let sharedFolderTreeQuery = $derived(createQuery({
		queryKey: ['shared-folder-tree', resourceId],
		queryFn: () => getSharedFolderTree(resourceId),
		enabled: resourceType === 'folder' && !!resourceId
	}));

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

	function navigateToSharedFolder(folderId: string) {
		if (!rootFolderId) return;
		goto(sharedResourcePath('folder', rootFolderId, { folderId }), {
			keepFocus: true,
			noScroll: true
		});
	}

	function navigateToRootFolder() {
		if (!rootFolderId) return;
		navigateToSharedFolder(rootFolderId);
	}

	function navigateToNestedFolder(index: number) {
		const target = nestedPath[index];
		if (!target) return;
		navigateToSharedFolder(target.id);
	}

	function openNestedFolder(folder: Folder) {
		navigateToSharedFolder(folder.id);
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

	async function copyCurrentLocationLink() {
		try {
			const relativePath =
				resourceType === 'folder' && rootFolderId
					? sharedResourcePath('folder', rootFolderId, { folderId: currentFolderId })
					: sharedResourcePath('file', resourceId);
			const url = `${window.location.origin}${relativePath}`;

			await navigator.clipboard.writeText(url);
			toastStore.show('Link copied to clipboard', 'success');
		} catch (error) {
			console.error('Failed to copy shared link', error);
			toastStore.show('Failed to copy link', 'error');
		}
	}

	let nestedPath = $derived(
		resourceType === 'folder' && currentFolderId
			? currentFolderId === rootFolderId
				? []
				: $sharedFolderTreeQuery.data
					? findFolderPathInTree($sharedFolderTreeQuery.data, currentFolderId).slice(1)
					: $currentFolderQuery.data
						? [$currentFolderQuery.data]
						: []
			: []
	);

	let currentFolderTitle = $derived(
		nestedPath.length > 0
			? nestedPath[nestedPath.length - 1].name
			: $currentFolderQuery.data?.name ?? shareEntry?.resource_name
	);

	let visibleFolders = $derived(
		$folderContentsQuery.data
			? filterUserVisibleEntries($folderContentsQuery.data.folders ?? [])
			: []
	);
	let visibleFiles = $derived(
		$folderContentsQuery.data
			? filterUserVisibleEntries($folderContentsQuery.data.files ?? [])
			: []
	);
</script>

<svelte:head>
	<title>
		{shareEntry
			? `${shareEntry.resource_name} - Shared with Me - RustShare`
			: 'Shared Resource - RustShare'}
	</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center justify-between gap-4">
		<div>
			<button class="btn mb-2 -ml-2 btn-ghost btn-sm" onclick={() => goto('/shared-with-me')}>
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
		<div class="flex justify-center py-12">
			<span class="loading loading-lg loading-spinner"></span>
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
						<div class="flex justify-center py-12">
							<span class="loading loading-lg loading-spinner"></span>
						</div>
					{:else if $fileQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load file: {$fileQuery.error?.message}</span>
						</div>
					{:else if $fileQuery.data}
						<div class="space-y-5">
							<div class="flex items-start gap-4">
								<div class="flex h-14 w-14 items-center justify-center rounded-xl bg-brand-500/10">
									<FileIcon
										mimeType={$fileQuery.data.mime_type}
										size="lg"
										iconClass="text-brand-500"
									/>
								</div>
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
								<button class="btn btn-ghost" onclick={copyCurrentLocationLink}>
									Copy Link
								</button>
								<button class="btn btn-primary" onclick={() => openPreview($fileQuery.data)}>
									Preview
								</button>
								<button class="btn btn-outline" onclick={() => handleDownload($fileQuery.data)}>
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
					<div class="flex flex-wrap items-center justify-between gap-4">
						<div>
							<h2 class="card-title">Shared Folder</h2>
							<p class="text-sm text-base-content/70">
								Browse the contents of this shared folder without leaving the shared workspace
							</p>
						</div>
						<div class="badge badge-ghost">{permissionLabel(shareEntry.permission)}</div>
					</div>

					<div class="mt-4 text-sm text-base-content/70">
						<div>Shared by {shareEntry.shared_by_name}</div>
						<div>Original path: {shareEntry.resource_path}</div>
					</div>
				</div>
			</div>

			<div class="card bg-base-100 shadow-xl">
				<div class="card-body">
					<div class="flex flex-wrap items-center justify-between gap-4">
						<div>
							<h2 class="card-title">{currentFolderTitle}</h2>
							<div class="breadcrumbs text-sm">
								<ul>
									<li>
										<button type="button" class="link link-hover" onclick={navigateToRootFolder}>
											{shareEntry.resource_name}
										</button>
									</li>
									{#each nestedPath as folder, index}
										<li>
											<button
												type="button"
												class="link link-hover"
												onclick={() => navigateToNestedFolder(index)}
											>
												{folder.name}
											</button>
										</li>
									{/each}
								</ul>
							</div>
						</div>
						<button class="btn btn-outline btn-sm" onclick={copyCurrentLocationLink}>
							Copy Current Link
						</button>
					</div>

					{#if $folderContentsQuery.isLoading}
						<div class="flex justify-center py-12">
							<span class="loading loading-lg loading-spinner"></span>
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
										{#each visibleFolders as folder}
											<tr class="hover">
												<td>
													<button
														type="button"
														class="flex items-center gap-3 font-medium hover:text-primary"
														onclick={() => openNestedFolder(folder)}
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
														class="btn btn-outline btn-sm"
														onclick={() => openNestedFolder(folder)}
													>
														Open
													</button>
												</td>
											</tr>
										{/each}

										{#each visibleFiles as file}
											<tr class="hover">
												<td>
													<button
														type="button"
														class="flex items-center gap-3 font-medium hover:text-primary"
														onclick={() => openPreview(file)}
													>
														<FileIcon
															mimeType={file.mime_type}
															size="md"
															iconClass="text-base-content/70"
														/>
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
															class="btn btn-ghost btn-sm"
															onclick={() => openPreview(file)}
														>
															Preview
														</button>
														<button
															type="button"
															class="btn btn-outline btn-sm"
															onclick={() => handleDownload(file)}
														>
															Download
														</button>
													</div>
												</td>
											</tr>
										{/each}

										{#if visibleFolders.length === 0 && visibleFiles.length === 0}
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
	onClose={() => {
		showPreviewModal = false;
		previewFile = null;
	}}
/>
