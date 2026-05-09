<script lang="ts">
	import { createQuery, createMutation } from '$lib/query-compat';
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
	import { Users, UserPlus, Link, Loader as Loader2, Trash2, X, Mail, User } from 'lucide-svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';

	interface Props {
		open?: boolean;
		resourceId?: string;
		resourceName?: string;
		resourceType?: 'file' | 'folder';
		onClose?: () => void;
		onNotification?: (payload: { message: string; type: 'success' | 'error' | 'info' }) => void;
	}

	let {
		open = false,
		resourceId = '',
		resourceName = '',
		resourceType = 'file',
		onClose = () => {},
		onNotification = () => {}
	}: Props = $props();

	let showConfirmModal = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmDanger = $state(false);
	let confirmOnConfirm = $state<() => void>(() => {});

	// Form state for new share
	let permissions: 'View' | 'Edit' | 'Admin' = $state('View');
	let password = $state('');
	let expiresAt = $state('');
	let uploadOnly = $state(false);
	let activeTab: 'public' | 'share' = $state('public');
	let recipientPermissionDrafts: Record<string, 'View' | 'Edit' | 'Admin'> = $state({});

	// User sharing state
	let recipientEmail = $state('');
	let userPermission: 'View' | 'Edit' | 'Admin' = $state('View');

	// Group sharing state
	let selectedGroupId = $state('');
	let groupPermission: 'View' | 'Edit' | 'Admin' = $state('View');

	// Query for existing shares
	let sharesQuery = $derived(
		createQuery({
			queryKey: ['public-shares', resourceType, resourceId],
			queryFn: () =>
				resourceType === 'folder' ? listFolderShares(resourceId) : listFileShares(resourceId),
			enabled: open
		})
	);

	let recipientsQuery = $derived(
		createQuery({
			queryKey: ['share-recipients', resourceType, resourceId],
			queryFn: () =>
				resourceType === 'folder'
					? listFolderRecipients(resourceId)
					: listFileRecipients(resourceId),
			enabled: open
		})
	);

	// Query for user's groups
	let groupsQuery = $derived(
		createQuery({
			queryKey: ['my-groups'],
			queryFn: listMyGroups,
			enabled: open && activeTab === ('share' as any)
		})
	);

	// Query for existing group shares
	let groupSharesQuery = $derived(
		createQuery({
			queryKey: ['group-shares', resourceType, resourceId],
			queryFn: () =>
				resourceType === 'folder'
					? listFolderGroupShares(resourceId)
					: listFileGroupShares(resourceId),
			enabled: open && activeTab === ('share' as any)
		})
	);

	// Mutation for creating public share
	const createShareMutation = createMutation({
		mutationFn: async (request: CreateShareRequest) => {
			return createShare(resourceType, resourceId, request);
		},
		onSuccess: (response) => {
			queryClient.invalidateQueries({ queryKey: ['public-shares', resourceType, resourceId] });
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			onNotification({
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
			onNotification({
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			onNotification({
				message: `Shared with ${recipientEmail.trim()}`,
				type: 'success'
			});
			recipientEmail = '';
			userPermission = 'View';
		},
		onError: (error) => {
			onNotification({
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
			onNotification({
				message: `Shared with group "${data.group_name}"`,
				type: 'success'
			});
			selectedGroupId = '';
			groupPermission = 'View';
		},
		onError: (error) => {
			onNotification({
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			onNotification({
				message: 'Access revoked successfully',
				type: 'success'
			});
		},
		onError: (error) => {
			onNotification({
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			onNotification({
				message: 'Permission updated',
				type: 'success'
			});
		},
		onError: (error) => {
			onNotification({
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
			queryClient.invalidateQueries({ queryKey: ['file-workspace'] });
			queryClient.invalidateQueries({ queryKey: ['folder-tree'] });
			onNotification({
				message: 'Access removed successfully',
				type: 'success'
			});
		},
		onError: (error) => {
			onNotification({
				message: error instanceof Error ? error.message : 'Failed to remove access',
				type: 'error'
			});
		}
	});

	function handleCopyLink(url: string) {
		navigator.clipboard
			.writeText(url)
			.then(() => {
				onNotification({
					message: 'Share link copied to clipboard',
					type: 'success'
				});
			})
			.catch(() => {
				onNotification({
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
			onNotification({
				message: 'Please enter an email address',
				type: 'error'
			});
			return;
		}
		$createUserShareMutation.mutate();
	}

	function handleShareWithGroup() {
		if (!selectedGroupId) {
			onNotification({
				message: 'Please select a group',
				type: 'error'
			});
			return;
		}
		$createGroupShareMutation.mutate();
	}

	function handleRevoke(shareId: string, type: 'public' | 'group') {
		confirmTitle = type === 'public' ? 'Revoke Share Link' : 'Remove Group Access';
		confirmMessage =
			type === 'public'
				? 'Are you sure you want to revoke this share link?'
				: "Are you sure you want to remove this group's access?";
		confirmDanger = true;
		confirmOnConfirm = () => {
			$revokeShareMutation.mutate(shareId);
		};
		showConfirmModal = true;
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
		confirmTitle = 'Remove Access';
		confirmMessage = `Remove access for "${recipient.email}"?`;
		confirmDanger = true;
		confirmOnConfirm = () => {
			$removeRecipientMutation.mutate(recipient.share_id);
		};
		showConfirmModal = true;
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
		onClose();
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

	$effect(() => {
		if ($recipientsQuery.data) {
			const nextDrafts: Record<string, 'View' | 'Edit' | 'Admin'> = {};
			for (const recipient of $recipientsQuery.data) {
				nextDrafts[recipient.share_id] =
					recipientPermissionDrafts[recipient.share_id] || recipient.permission;
			}
			if (recipientDraftsChanged(nextDrafts)) {
				recipientPermissionDrafts = nextDrafts;
			}
		}
	});

	let isLoading = $derived(
		$createShareMutation.isPending ||
			$revokeShareMutation.isPending ||
			$createUserShareMutation.isPending ||
			$createGroupShareMutation.isPending ||
			$updateRecipientPermissionMutation.isPending ||
			$removeRecipientMutation.isPending
	);
</script>

<dialog class="modal" class:modal-open={open}>
	<div class="modal-box max-w-2xl">
		<h3 class="mb-4 text-lg font-bold">Share "{resourceName}"</h3>

		<div class="tabs-boxed mb-6 tabs">
			<button
				type="button"
				class:tab-active={activeTab === 'public'}
				class="tab"
				onclick={() => (activeTab = 'public')}
			>
				<Link class="mr-1 h-4 w-4" />
				Link
			</button>
			<button
				type="button"
				class:tab-active={activeTab === 'share'}
				class="tab"
				onclick={() => (activeTab = 'share')}
			>
				<UserPlus class="mr-1 h-4 w-4" />
				Share
			</button>
		</div>

		{#if activeTab === 'public'}
			<!-- Create new share form -->
			<div class="mb-6">
				<div class="card bg-base-200 p-4">
					<h4 class="mb-3 font-semibold">Create Public Share Link</h4>

					<form
						onsubmit={(e) => {
							e.preventDefault();
							handleCreateShare();
						}}
						class="space-y-4"
					>
						<!-- Permission selector -->
						<div class="form-control">
							<label class="label" for="permissions">
								<span class="label-text">Permissions</span>
							</label>
							<select
								id="permissions"
								class="select-bordered select"
								bind:value={permissions}
								disabled={isLoading || (resourceType === 'folder' && uploadOnly)}
							>
								<option value="View">
									{resourceType === 'folder' ? 'View & Download' : 'View Only (Read, No Download)'}
								</option>
								<option value="Edit">
									{resourceType === 'folder' ? 'View & Upload' : 'View & Download'}
								</option>
								<option value="Admin">Full Access (View, Download, Manage)</option>
							</select>
							{#if resourceType === 'folder'}
								<label class="label mt-2 cursor-pointer justify-start gap-3">
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
									<p class="mt-1 text-xs text-base-content/60">
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
								class="input-bordered input"
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
								class="input-bordered input"
								bind:value={expiresAt}
								disabled={isLoading}
							/>
						</div>

						<div class="flex justify-end">
							<button type="submit" class="btn btn-primary" disabled={isLoading}>
								{#if $createShareMutation.isPending}
									<span class="loading mr-2 loading-sm loading-spinner"></span>
								{/if}
								Generate Link
							</button>
						</div>
					</form>
				</div>
			</div>

			<!-- List existing public shares -->
			<div>
				<h4 class="mb-3 font-semibold">Existing Share Links</h4>

				{#if $sharesQuery.isLoading}
					<div class="flex justify-center py-8">
						<span class="loading loading-md loading-spinner"></span>
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
									<div class="flex items-start justify-between gap-4">
										<div class="min-w-0 flex-1">
											{#if share.share_token}
												<!-- Share URL -->
												<div class="mb-2 flex items-center gap-2">
													<input
														type="text"
														class="input-bordered input input-sm flex-1 font-mono text-sm"
														value={getShareUrl(share.share_token)}
														readonly
													/>
													<button
														type="button"
														class="btn btn-ghost btn-sm"
														onclick={() => handleCopyLink(getShareUrl(share.share_token)!)}
														title="Copy to clipboard"
													>
														<svg
															xmlns="http://www.w3.org/2000/svg"
															fill="none"
															viewBox="0 0 24 24"
															stroke-width="1.5"
															stroke="currentColor"
															class="h-4 w-4"
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
											<div class="space-y-1 text-sm text-base-content/70">
												<div class="flex flex-wrap gap-4">
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
											onclick={() => handleRevoke(share.id, 'public')}
											disabled={isLoading}
										>
											{#if $revokeShareMutation.isPending}
												<span class="loading loading-xs loading-spinner"></span>
											{/if}
											Revoke
										</button>
									</div>
								</div>
							</div>
						{/each}
					</div>
				{:else}
					<div class="py-8 text-center text-base-content/60">
						<p>No active share links for this {resourceType}</p>
					</div>
				{/if}
			</div>
		{:else}
			<!-- Combined Share Tab - Users and Groups -->
			<div class="space-y-6">
				<!-- Share with User Section -->
				<div class="card bg-base-200 p-4">
					<h4 class="mb-3 flex items-center gap-2 font-semibold">
						<Mail class="h-4 w-4" />
						Share with a Person
					</h4>
					<form
						onsubmit={(e) => {
							e.preventDefault();
							handleShareWithUser();
						}}
						class="space-y-3"
					>
						<div class="flex gap-2">
							<input
								type="email"
								class="input-bordered input flex-1"
								placeholder="email@example.com"
								bind:value={recipientEmail}
								disabled={isLoading}
							/>
							<select
								class="select-bordered select w-28"
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
									<Loader2 class="h-4 w-4 animate-spin" />
								{:else}
									Share
								{/if}
							</button>
						</div>
					</form>
				</div>

				<!-- Share with Group Section -->
				<div class="card bg-base-200 p-4">
					<h4 class="mb-3 flex items-center gap-2 font-semibold">
						<Users class="h-4 w-4" />
						Share with a Group
					</h4>
					<form
						onsubmit={(e) => {
							e.preventDefault();
							handleShareWithGroup();
						}}
						class="space-y-3"
					>
						{#if $groupsQuery.isLoading}
							<div class="flex items-center gap-2 py-2 text-base-content/60">
								<Loader2 class="h-4 w-4 animate-spin" />
								<span>Loading groups...</span>
							</div>
						{:else if $groupsQuery.isError}
							<div class="alert-sm alert alert-error">
								<span>Failed to load groups</span>
							</div>
						{:else if $groupsQuery.data && $groupsQuery.data.length > 0}
							<div class="flex gap-2">
								<select
									class="select-bordered select flex-1"
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
									class="select-bordered select w-28"
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
										<Loader2 class="h-4 w-4 animate-spin" />
									{:else}
										Share
									{/if}
								</button>
							</div>
						{:else}
							<div class="alert-sm alert alert-info">
								<span>You are not a member of any groups yet.</span>
							</div>
						{/if}
					</form>
				</div>

				<!-- Recipients and Group Shares List -->
				<div>
					<h4 class="mb-3 font-semibold">People with Access</h4>
					{#if $recipientsQuery.isLoading}
						<div class="flex justify-center py-8">
							<Loader2 class="h-6 w-6 animate-spin text-brand-500" />
						</div>
					{:else if $recipientsQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load recipients: {$recipientsQuery.error?.message}</span>
						</div>
					{:else if $recipientsQuery.data && $recipientsQuery.data.length > 0}
						<div class="mb-6 space-y-2">
							{#each $recipientsQuery.data as recipient}
								<div class="flex items-center justify-between rounded-lg bg-base-200 p-3">
									<div class="flex min-w-0 items-center gap-3">
										<div
											class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-brand-100"
										>
											<User class="h-4 w-4 text-brand-600" />
										</div>
										<div class="min-w-0">
											<div class="truncate font-medium">{recipient.email}</div>
											<div class="text-xs text-base-content/60">
												Added {formatDate(recipient.added_at)}
											</div>
										</div>
									</div>
									<div class="flex items-center gap-2">
										<select
											class="select-bordered select select-sm"
											value={currentRecipientPermission(recipient)}
											disabled={isLoading}
											onchange={(event) =>
												handleRecipientPermissionSelect(recipient.share_id, event)}
										>
											<option value="View">View</option>
											<option value="Edit">Edit</option>
											<option value="Admin">Admin</option>
										</select>
										<button
											type="button"
											class="btn btn-ghost btn-sm"
											onclick={() => handleSaveRecipientPermission(recipient)}
											disabled={isLoading ||
												currentRecipientPermission(recipient) === recipient.permission}
										>
											Save
										</button>
										<button
											type="button"
											class="btn btn-ghost btn-sm btn-error"
											onclick={() => handleRemoveRecipient(recipient)}
											disabled={isLoading}
											title="Remove access"
										>
											<Trash2 class="h-4 w-4" />
										</button>
									</div>
								</div>
							{/each}
						</div>
					{:else}
						<p class="mb-6 text-sm text-base-content/60">No individual users have access yet.</p>
					{/if}

					<h4 class="mb-3 font-semibold">Groups with Access</h4>
					{#if $groupSharesQuery.isLoading}
						<div class="flex justify-center py-8">
							<Loader2 class="h-6 w-6 animate-spin text-brand-500" />
						</div>
					{:else if $groupSharesQuery.isError}
						<div class="alert alert-error">
							<span>Failed to load group shares: {$groupSharesQuery.error?.message}</span>
						</div>
					{:else if $groupSharesQuery.data && $groupSharesQuery.data.length > 0}
						<div class="space-y-2">
							{#each $groupSharesQuery.data as groupShare}
								<div class="flex items-center justify-between rounded-lg bg-base-200 p-3">
									<div class="flex min-w-0 items-center gap-3">
										<div
											class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-brand-100"
										>
											<Users class="h-4 w-4 text-brand-600" />
										</div>
										<div class="min-w-0">
											<div class="truncate font-medium">{groupShare.group_name}</div>
											<div class="text-xs text-base-content/60">
												<span class="badge badge-xs">{groupShare.permission}</span>
												<span class="ml-2">Shared {formatDate(groupShare.created_at)}</span>
											</div>
										</div>
									</div>
									<button
										type="button"
										class="btn btn-ghost btn-sm btn-error"
										onclick={() => handleRevoke(groupShare.share_id, 'group')}
										disabled={isLoading}
										title="Remove group access"
									>
										<Trash2 class="h-4 w-4" />
									</button>
								</div>
							{/each}
						</div>
					{:else}
						<p class="text-sm text-base-content/60">No groups have access yet.</p>
					{/if}
				</div>
			</div>
		{/if}

		<div class="modal-action">
			<button type="button" class="btn" onclick={handleClose} disabled={isLoading}>Close</button>
		</div>
	</div>

	<ConfirmModal
		open={showConfirmModal}
		title={confirmTitle}
		message={confirmMessage}
		confirmLabel="Confirm"
		cancelLabel="Cancel"
		danger={confirmDanger}
		onConfirm={() => {
			showConfirmModal = false;
			confirmOnConfirm();
		}}
		onCancel={() => (showConfirmModal = false)}
	/>

	<form method="dialog" class="modal-backdrop">
		<button type="button" onclick={handleClose} disabled={isLoading}>close</button>
	</form>
</dialog>
