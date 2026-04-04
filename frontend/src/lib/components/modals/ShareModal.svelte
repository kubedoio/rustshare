<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import {
		createFileUserShare,
		createFolderUserShare,
		createShare,
		listFolderShares,
		listFolderRecipients,
		listFileRecipients,
		listFileShares,
		removeShareRecipient,
		revokeShare,
		updateSharePermission
	} from '$lib/api/shares';
	import {
		listMyGroups,
		createFileGroupShare,
		createFolderGroupShare,
		listFileGroupShares,
		listFolderGroupShares,
		type Group,
		type GroupShareResponse
	} from '$lib/api/groups';
	import type { CreateShareRequest } from '$lib/api/shares';
	import type { Share, ShareRecipient } from '$lib/api/types';
	import { queryClient } from '$lib/query-client';
	import { formatDate } from '$lib/utils/format';
	import { createEventDispatcher } from 'svelte';
	import { Users, UserPlus, Link, Loader2, Trash2, X, Mail, User } from 'lucide-svelte';

	export let open = false;
	export let resourceId: string;
	export let resourceName: string;
	export let resourceType: 'file' | 'folder' = 'file';

	type DispatchEvents = {
		close: void;
		notification: { message: string; type: 'success' | 'error' | 'info' };
	}
	const dispatch = createEventDispatcher<DispatchEvents>();

	// Form state for new share
	let permissions: 'View' | 'Edit' | 'Admin' = 'View';
	let password = '';
	let expiresAt = '';
	let uploadOnly = false;
	let activeTab: 'public' | 'share' = 'public';
	let recipientPermissionDrafts: Record<string, 'View' | 'Edit' | 'Admin'> = {};

	// User sharing state
	let recipientEmail = '';
	let userPermission: 'View' | 'Edit' | 'Admin' = 'View';

	// Group sharing state
	let selectedGroupId = '';
	let groupPermission: 'View' | 'Edit' | 'Admin' = 'View';

	// Query for existing shares
	$: sharesQuery = createQuery({
		queryKey: ['public-shares', resourceType, resourceId],
		queryFn: () =>
			resourceType === 'folder' ? listFolderShares(resourceId) : listFileShares(resourceId),
		enabled: open
	});

	$: recipientsQuery = createQuery({
		queryKey: ['share-recipients', resourceType, resourceId],
		queryFn: () =>
			resourceType === 'folder' ? listFolderRecipients(resourceId) : listFileRecipients(resourceId),
		enabled: open
	});

	// Query for user's groups
	$: groupsQuery = createQuery({
		queryKey: ['my-groups'],
		queryFn: listMyGroups,
		enabled: open && activeTab === 'share'
	});

	// Query for existing group shares
	$: groupSharesQuery = createQuery({
		queryKey: ['group-shares', resourceType, resourceId],
		queryFn: () =>
			resourceType === 'folder'
				? listFolderGroupShares(resourceId)
				: listFileGroupShares(resourceId),
		enabled: open && activeTab === 'share'
	});

	// Mutation for creating public share
	const createShareMutation = createMutation({
		mutationFn: async (request: CreateShareRequest) => {
			return createShare(resourceType, resourceId, request);
		},
		onSuccess: (response) => {
			queryClient.invalidateQueries({ queryKey: ['public-shares', resourceType, resourceId] });
			dispatch('notification', {
				message: 'Share link created successfully',
				type: 'success'
			});
			permissions = 'View';
			password = '';
			expiresAt = '';
			uploadOnly = false;
			handleCopyLink(response.share_url);
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to create share',
				type: 'error'
			});
		}
	});

	// Mutation for creating user share
	const createUserShareMutation = createMutation({
		mutationFn: async () => {
			if (resourceType === 'folder') {
				return createFolderUserShare(resourceId, {
					recipient_email: recipientEmail.trim(),
					permission: userPermission
				});
			}
			return createFileUserShare(resourceId, {
				recipient_email: recipientEmail.trim(),
				permission: userPermission
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['share-recipients', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['received-shares'] });
			dispatch('notification', {
				message: `Shared with ${recipientEmail.trim()}`,
				type: 'success'
			});
			recipientEmail = '';
			userPermission = 'View';
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to share',
				type: 'error'
			});
		}
	});

	// Mutation for creating group share
	const createGroupShareMutation = createMutation({
		mutationFn: async () => {
			const request = {
				group_id: selectedGroupId,
				permission: groupPermission
			};
			if (resourceType === 'folder') {
				return createFolderGroupShare(resourceId, request);
			}
			return createFileGroupShare(resourceId, request);
		},
		onSuccess: (data) => {
			queryClient.invalidateQueries({ queryKey: ['group-shares', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['received-shares'] });
			dispatch('notification', {
				message: `Shared with group "${data.group_name}"`,
				type: 'success'
			});
			selectedGroupId = '';
			groupPermission = 'View';
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to share with group',
				type: 'error'
			});
		}
	});

	// Mutation for revoking share
	const revokeShareMutation = createMutation({
		mutationFn: async (shareId: string) => {
			return revokeShare(shareId);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['public-shares', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['group-shares', resourceType, resourceId] });
			dispatch('notification', {
				message: 'Access revoked successfully',
				type: 'success'
			});
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to revoke access',
				type: 'error'
			});
		}
	});

	const updateRecipientPermissionMutation = createMutation({
		mutationFn: async (payload: { shareId: string; permission: 'View' | 'Edit' | 'Admin' }) => {
			return updateSharePermission(payload.shareId, {
				permission: payload.permission
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['share-recipients', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['received-shares'] });
			dispatch('notification', {
				message: 'Permission updated',
				type: 'success'
			});
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to update permission',
				type: 'error'
			});
		}
	});

	const removeRecipientMutation = createMutation({
		mutationFn: async (shareId: string) => {
			return removeShareRecipient(shareId);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['share-recipients', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['received-shares'] });
			dispatch('notification', {
				message: 'Access removed successfully',
				type: 'success'
			});
		},
		onError: (error) => {
			dispatch('notification', {
				message: error instanceof Error ? error.message : 'Failed to remove access',
				type: 'error'
			});
		}
	});

	function handleCopyLink(url: string) {
		navigator.clipboard
			.writeText(url)
			.then(() => {
				dispatch('notification', {
					message: 'Share link copied to clipboard',
					type: 'success'
				});
			})
			.catch(() => {
				dispatch('notification', {
					message: 'Failed to copy link',
					type: 'error'
				});
			});
	}

	function handleCreateShare() {
		const request: CreateShareRequest = {
			permissions: resourceType === 'folder' && uploadOnly ? 'View' : permissions
		};

		if (password.trim()) {
			request.password = password.trim();
		}

		if (expiresAt) {
			request.expires_at = new Date(expiresAt).toISOString();
		}

		if (resourceType === 'folder' && uploadOnly) {
			request.upload_only = true;
		}

		$createShareMutation.mutate(request);
	}

	function handleShareWithUser() {
		if (!recipientEmail.trim()) {
			dispatch('notification', {
				message: 'Please enter an email address',
				type: 'error'
			});
			return;
		}
		$createUserShareMutation.mutate();
	}

	function handleShareWithGroup() {
		if (!selectedGroupId) {
			dispatch('notification', {
				message: 'Please select a group',
				type: 'error'
			});
			return;
		}
		$createGroupShareMutation.mutate();
	}

	function handleRevoke(shareId: string, type: 'public' | 'group') {
		const message = type === 'public' 
			? 'Are you sure you want to revoke this share link?'
			: 'Are you sure you want to remove this group\'s access?';
		if (confirm(message)) {
			$revokeShareMutation.mutate(shareId);
		}
	}

	function handleRecipientPermissionChange(shareId: string, permission: 'View' | 'Edit' | 'Admin') {
		recipientPermissionDrafts = {
			...recipientPermissionDrafts,
			[shareId]: permission
		};
	}

	function handleRecipientPermissionSelect(shareId: string, event: Event) {
		const permission = (event.currentTarget as HTMLSelectElement).value as
			| 'View'
			| 'Edit'
			| 'Admin';
		handleRecipientPermissionChange(shareId, permission);
	}

	function currentRecipientPermission(recipient: ShareRecipient): 'View' | 'Edit' | 'Admin' {
		return recipientPermissionDrafts[recipient.share_id] || recipient.permission;
	}

	function recipientDraftsChanged(nextDrafts: Record<string, 'View' | 'Edit' | 'Admin'>): boolean {
		const currentKeys = Object.keys(recipientPermissionDrafts);
		const nextKeys = Object.keys(nextDrafts);

		if (currentKeys.length !== nextKeys.length) {
			return true;
		}

		return nextKeys.some((key) => recipientPermissionDrafts[key] !== nextDrafts[key]);
	}

	function handleSaveRecipientPermission(recipient: ShareRecipient) {
		const permission = currentRecipientPermission(recipient);
		if (permission === recipient.permission) {
			return;
		}

		$updateRecipientPermissionMutation.mutate({
			shareId: recipient.share_id,
			permission
		});
	}

	function handleRemoveRecipient(recipient: ShareRecipient) {
		if (confirm(`Remove access for "${recipient.email}"?`)) {
			$removeRecipientMutation.mutate(recipient.share_id);
		}
	}

	function handleClose() {
		activeTab = 'public';
		permissions = 'View';
		password = '';
		expiresAt = '';
		uploadOnly = false;
		recipientEmail = '';
		userPermission = 'View';
		selectedGroupId = '';
		groupPermission = 'View';
		dispatch('close');
	}

	function getShareUrl(token: string | null): string | null {
		if (!token) return null;
		const baseUrl = window.location.origin;
		return `${baseUrl}/share/${token}`;
	}

	function getGroupName(groupId: string, groups: Group[] = []): string {
		const group = groups.find((g) => g.id === groupId);
		return group?.name || 'Unknown Group';
	}

	$: if ($recipientsQuery.data) {
		const nextDrafts: Record<string, 'View' | 'Edit' | 'Admin'> = {};
		for (const recipient of $recipientsQuery.data) {
			nextDrafts[recipient.share_id] =
				recipientPermissionDrafts[recipient.share_id] || recipient.permission;
		}
		if (recipientDraftsChanged(nextDrafts)) {
			recipientPermissionDrafts = nextDrafts;
		}
	}

	$: isLoading =
		$createShareMutation.isPending ||
		$revokeShareMutation.isPending ||
		$createUserShareMutation.isPending ||
		$createGroupShareMutation.isPending ||
		$updateRecipientPermissionMutation.isPending ||
		$removeRecipientMutation.isPending;
</script>

<dialog class="modal" class:modal-open={open}>
	<div class="modal-box max-w-2xl">
		<h3 class="font-bold text-lg mb-4">Share "{resourceName}"</h3>

		<div class="tabs tabs-boxed mb-6">
			<button
				type="button"
				class:tab-active={activeTab === 'public'}
				class="tab"
				on:click={() => (activeTab = 'public')}
			>
				<Link class="w-4 h-4 mr-1" />
				Link
			</button>
			<button
				type="button"
				class:tab-active={activeTab === 'share'}
				class="tab"
				on:click={() => (activeTab = 'share')}
			>
				<UserPlus class="w-4 h-4 mr-1" />
				Share
			</button>
		</div>

		{#if activeTab === 'public'}
			<!-- Create new share form -->
			<div class="mb-6">
				<div class="card bg-base-200 p-4">
					<h4 class="font-semibold mb-3">Create Public Share Link</h4>

					<form on:submit|preventDefault={handleCreateShare} class="space-y-4">
						<!-- Permission selector -->
						<div class="form-control">
							<label class="label" for="permissions">
								<span class="label-text">Permissions</span>
							</label>
							<select
								id="permissions"
								class="select select-bordered"
								bind:value={permissions}
								disabled={isLoading || (resourceType === 'folder' && uploadOnly)}
							>
								<option value="View">
									{resourceType === 'folder'
										? 'View & Download'
										: 'View Only (Read, No Download)'}
								</option>
								<option value="Edit">
									{resourceType === 'folder' ? 'View & Upload' : 'View & Download'}
								</option>
								<option value="Admin">Full Access (View, Download, Manage)</option>
							</select>
							{#if resourceType === 'folder'}
								<label class="label gap-3 mt-2 cursor-pointer justify-start">
									<input
										type="checkbox"
										class="checkbox checkbox-sm"
										bind:checked={uploadOnly}
										disabled={isLoading}
									/>
									<span class="label-text">
										Upload only (allow uploads without browsing or downloading existing files)
									</span>
								</label>
								{#if uploadOnly}
									<p class="text-xs text-base-content/60 mt-1">
										Upload-only links are restricted drop points. They always hide folder contents
										and file downloads from recipients.
									</p>
								{/if}
							{/if}
						</div>

						<!-- Optional password -->
						<div class="form-control">
							<label class="label" for="password">
								<span class="label-text">Password (optional)</span>
							</label>
							<input
								id="password"
								type="password"
								placeholder="Leave empty for no password"
								class="input input-bordered"
								bind:value={password}
								disabled={isLoading}
							/>
						</div>

						<!-- Optional expiry date -->
						<div class="form-control">
							<label class="label" for="expires-at">
								<span class="label-text">Expires At (optional)</span>
							</label>
							<input
								id="expires-at"
								type="datetime-local"
								class="input input-bordered"
								bind:value={expiresAt}
								disabled={isLoading}
							/>
						</div>

						<div class="flex justify-end">
							<button type="submit" class="btn btn-primary" disabled={isLoading}>
								{#if $createShareMutation.isPending}
									<span class="loading loading-spinner loading-sm mr-2"></span>
								{/if}
								Generate Link
							</button>
						</div>
					</form>
				</div>
			</div>

			<!-- List existing public shares -->
			<div>
				<h4 class="font-semibold mb-3">Existing Share Links</h4>

				{#if $sharesQuery.isLoading}
					<div class="py-8 flex justify-center">
						<span class="loading loading-spinner loading-md"></span>
					</div>
				{:else if $sharesQuery.isError}
					<div class="alert alert-error">
						<span>Failed to load shares: {$sharesQuery.error?.message}</span>
					</div>
				{:else if $sharesQuery.data && $sharesQuery.data.length > 0}
					<div class="space-y-3">
						{#each $sharesQuery.data as share}
							<div class="card bg-base-200">
								<div class="card-body p-4">
									<div class="gap-4 flex items-start justify-between">
										<div class="min-w-0 flex-1">
												{#if share.share_token}
											<!-- Share URL -->
											<div class="gap-2 mb-2 flex items-center">
												<input
													type="text"
													class="input input-bordered input-sm font-mono text-sm flex-1"
													value={getShareUrl(share.share_token)}
													readonly
												/>
												<button
													type="button"
													class="btn btn-sm btn-ghost"
													on:click={() => handleCopyLink(getShareUrl(share.share_token)!)}
													title="Copy to clipboard"
												>
													<svg
														xmlns="http://www.w3.org/2000/svg"
														fill="none"
														viewBox="0 0 24 24"
														stroke-width="1.5"
														stroke="currentColor"
														class="w-4 h-4"
													>
														<path
															stroke-linecap="round"
															stroke-linejoin="round"
															d="M15.666 3.888A2.25 2.25 0 0013.5 2.25h-3c-1.03 0-1.9.693-2.166 1.638m7.332 0c.055.194.084.4.084.612v0a.75.75 0 01-.75.75H9a.75.75 0 01-.75-.75v0c0-.212.03-.418.084-.612m7.332 0c.646.049 1.288.11 1.927.184 1.1.128 1.907 1.077 1.907 2.185V19.5a2.25 2.25 0 01-2.25 2.25H6.75A2.25 2.25 0 014.5 19.5V6.257c0-1.108.806-2.057 1.907-2.185a48.208 48.208 0 011.927-.184"
														/>
													</svg>
												</button>
											</div>

											{/if}
											<!-- Share details -->
											<div class="text-sm text-base-content/70 space-y-1">
												<div class="gap-4 flex flex-wrap">
													<span class="badge badge-sm">
														{share.permissions === 'View'
															? 'View Only'
															: share.permissions === 'Edit'
																? 'View & Download'
																: 'Full Access'}
													</span>
													{#if share.upload_only}
														<span class="badge badge-sm badge-warning">Upload Only</span>
													{/if}
													{#if share.password_protected}
														<span class="badge badge-sm badge-warning">Password Protected</span>
													{/if}
													{#if share.expires_at}
														<span class="badge badge-sm badge-error">
															Expires: {formatDate(share.expires_at)}
														</span>
													{:else}
														<span class="badge badge-sm badge-success">Never Expires</span>
													{/if}
												</div>
												<p>Created: {formatDate(share.created_at)}</p>
											</div>
										</div>

										<!-- Revoke button -->
										<button
											type="button"
											class="btn btn-sm btn-error"
											on:click={() => handleRevoke(share.id, 'public')}
											disabled={isLoading}
										>
											{#if $revokeShareMutation.isPending}
												<span class="loading loading-spinner loading-xs"></span>
											{/if}
											Revoke
										</button>
									</div>
								</div>
							</div>
						{/each}
					</div>
				{:else}
					<div class="py-8 text-base-content/60 text-center">
						<p>No active share links for this {resourceType}</p>
					</div>
				{/if}
			</div>
		{:else}
			<!-- Combined Share Tab - Users and Groups -->
			<div class="space-y-6">
				<!-- Share with User Section -->
				<div class="card bg-base-200 p-4">
					<h4 class="font-semibold mb-3 flex items-center gap-2">
						<Mail class="w-4 h-4" />
						Share with a Person
					</h4>
					<form on:submit|preventDefault={handleShareWithUser} class="space-y-3">
						<div class="flex gap-2">
							<input
								type="email"
								class="input input-bordered flex-1"
								placeholder="email@example.com"
								bind:value={recipientEmail}
								disabled={isLoading}
							/>
							<select
								class="select select-bordered w-28"
								bind:value={userPermission}
								disabled={isLoading}
							>
								<option value="View">View</option>
								<option value="Edit">Edit</option>
								<option value="Admin">Admin</option>
							</select>
							<button 
								type="submit" 
								class="btn btn-primary"
								disabled={isLoading || !recipientEmail.trim()}
							>
								{#if $createUserShareMutation.isPending}
									<Loader2 class="w-4 h-4 animate-spin" />
								{:else}
									Share
								{/if}
							</button>
						</div>
					</form>
				</div>

				<!-- Share with Group Section -->
				<div class="card bg-base-200 p-4">
					<h4 class="font-semibold mb-3 flex items-center gap-2">
						<Users class="w-4 h-4" />
						Share with a Group
					</h4>
					<form on:submit|preventDefault={handleShareWithGroup} class="space-y-3">
						{#if $groupsQuery.isLoading}
							<div class="flex items-center gap-2 py-2 text-base-content/60">
								<Loader2 class="w-4 h-4 animate-spin" />
								<span>Loading groups...</span>
							</div>
						{:else if $groupsQuery.isError}
							<div class="alert alert-error alert-sm">
								<span>Failed to load groups</span>
							</div>
						{:else if $groupsQuery.data && $groupsQuery.data.length > 0}
							<div class="flex gap-2">
								<select
									class="select select-bordered flex-1"
									bind:value={selectedGroupId}
									disabled={isLoading}
								>
									<option value="">Select a group...</option>
									{#each $groupsQuery.data as group}
										<option value={group.id}>
											{group.name} ({group.member_count} members)
										</option>
									{/each}
								</select>
								<select
									class="select select-bordered w-28"
									bind:value={groupPermission}
									disabled={isLoading}
								>
									<option value="View">View</option>
									<option value="Edit">Edit</option>
									<option value="Admin">Admin</option>
								</select>
								<button 
									type="submit" 
									class="btn btn-primary"
									disabled={isLoading || !selectedGroupId}
								>
									{#if $createGroupShareMutation.isPending}
										<Loader2 class="w-4 h-4 animate-spin" />
									{:else}
										Share
									{/if}
								</button>
							</div>
						{:else}
							<div class="alert alert-info alert-sm">
								<span>You are not a member of any groups yet.</span>
							</div>
						{/if}
					</form>
				</div>

				<!-- Recipients and Group Shares List -->
				<div>
					<h4 class="font-semibold mb-3">People with Access</h4>
					{#if $recipientsQuery.isLoading}
						<div class="py-8 flex justify-center">
							<Loader2 class="w-6 h-6 animate-spin text-brand-500" />
						</div>
					{:else if $recipientsQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load recipients: {$recipientsQuery.error?.message}</span>
						</div>
					{:else if $recipientsQuery.data && $recipientsQuery.data.length > 0}
						<div class="space-y-2 mb-6">
							{#each $recipientsQuery.data as recipient}
								<div class="flex items-center justify-between p-3 bg-base-200 rounded-lg">
									<div class="flex items-center gap-3 min-w-0">
										<div class="w-8 h-8 rounded-full bg-brand-100 flex items-center justify-center flex-shrink-0">
											<User class="w-4 h-4 text-brand-600" />
										</div>
										<div class="min-w-0">
											<div class="font-medium truncate">{recipient.email}</div>
											<div class="text-xs text-base-content/60">
												Added {formatDate(recipient.added_at)}
											</div>
										</div>
									</div>
									<div class="flex items-center gap-2">
										<select
											class="select select-bordered select-sm"
											value={currentRecipientPermission(recipient)}
											disabled={isLoading}
											on:change={(event) =>
												handleRecipientPermissionSelect(recipient.share_id, event)}
										>
											<option value="View">View</option>
											<option value="Edit">Edit</option>
											<option value="Admin">Admin</option>
										</select>
										<button
											type="button"
											class="btn btn-sm btn-ghost"
											on:click={() => handleSaveRecipientPermission(recipient)}
											disabled={isLoading ||
												currentRecipientPermission(recipient) === recipient.permission}
										>
											Save
										</button>
										<button
											type="button"
											class="btn btn-sm btn-error btn-ghost"
											on:click={() => handleRemoveRecipient(recipient)}
											disabled={isLoading}
											title="Remove access"
										>
											<Trash2 class="w-4 h-4" />
										</button>
									</div>
								</div>
							{/each}
						</div>
					{:else}
						<p class="text-base-content/60 text-sm mb-6">No individual users have access yet.</p>
					{/if}

					<h4 class="font-semibold mb-3">Groups with Access</h4>
					{#if $groupSharesQuery.isLoading}
						<div class="py-8 flex justify-center">
							<Loader2 class="w-6 h-6 animate-spin text-brand-500" />
						</div>
					{:else if $groupSharesQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load group shares: {$groupSharesQuery.error?.message}</span>
						</div>
					{:else if $groupSharesQuery.data && $groupSharesQuery.data.length > 0}
						<div class="space-y-2">
							{#each $groupSharesQuery.data as groupShare}
								<div class="flex items-center justify-between p-3 bg-base-200 rounded-lg">
									<div class="flex items-center gap-3 min-w-0">
										<div class="w-8 h-8 rounded-full bg-brand-100 flex items-center justify-center flex-shrink-0">
											<Users class="w-4 h-4 text-brand-600" />
										</div>
										<div class="min-w-0">
											<div class="font-medium truncate">{groupShare.group_name}</div>
											<div class="text-xs text-base-content/60">
												<span class="badge badge-xs">{groupShare.permission}</span>
												<span class="ml-2">Shared {formatDate(groupShare.created_at)}</span>
											</div>
										</div>
									</div>
									<button
										type="button"
										class="btn btn-sm btn-error btn-ghost"
										on:click={() => handleRevoke(groupShare.share_id, 'group')}
										disabled={isLoading}
										title="Remove group access"
									>
										<Trash2 class="w-4 h-4" />
									</button>
								</div>
							{/each}
						</div>
					{:else}
						<p class="text-base-content/60 text-sm">No groups have access yet.</p>
					{/if}
				</div>
			</div>
		{/if}

		<div class="modal-action">
			<button type="button" class="btn" on:click={handleClose} disabled={isLoading}>Close</button>
		</div>
	</div>

	<form method="dialog" class="modal-backdrop">
		<button type="button" on:click={handleClose} disabled={isLoading}>close</button>
	</form>
</dialog>
