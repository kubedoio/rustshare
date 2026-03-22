<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { getShareAccessLog, listAllUserShares, revokeShare } from '$lib/api/shares';
	import { queryClient } from '$lib/query-client';
	import type { Share, ShareAccessLogEntry } from '$lib/api/types';
	import Toast from '$lib/components/common/Toast.svelte';

	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';
	let activeShareActivityId: string | null = null;
	let shareActivity: ShareAccessLogEntry[] = [];
	let shareActivityLoading = false;
	let shareActivityError = '';

	// Query for all shares
	const sharesQuery = createQuery({
		queryKey: ['user-shares'],
		queryFn: listAllUserShares
	});

	// Revoke share mutation
	const revokeShareMutation = createMutation({
		mutationFn: revokeShare,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['user-shares'] });
			displayToast('Share link revoked successfully', 'success');
		},
		onError: (error: Error) => {
			displayToast(`Failed to revoke share: ${error.message}`, 'error');
		}
	});

	function displayToast(message: string, type: 'success' | 'error' | 'info') {
		toastMessage = message;
		toastType = type;
		showToast = true;
		setTimeout(() => {
			showToast = false;
		}, 3000);
	}

	function getShareUrl(token: string): string {
		const baseUrl = window.location.origin;
		return `${baseUrl}/share/${token}`;
	}

	function copyShareLink(token: string) {
		const url = getShareUrl(token);
		navigator.clipboard.writeText(url);
		displayToast('Share link copied to clipboard', 'success');
	}

	async function toggleShareActivity(share: Share) {
		if (activeShareActivityId === share.id) {
			closeShareActivity();
			return;
		}

		activeShareActivityId = share.id;
		shareActivity = [];
		shareActivityError = '';
		shareActivityLoading = true;

		try {
			shareActivity = await getShareAccessLog(share.id, 50);
		} catch (error) {
			shareActivityError = error instanceof Error ? error.message : 'Failed to load activity';
		} finally {
			shareActivityLoading = false;
		}
	}

	function closeShareActivity() {
		activeShareActivityId = null;
		shareActivity = [];
		shareActivityError = '';
		shareActivityLoading = false;
	}

	function handleRevokeShare(share: Share) {
		if (
			confirm(`Revoke share link for "${share.resource_name || `this ${share.resource_type}`}"?`)
		) {
			$revokeShareMutation.mutate(share.id);
		}
	}

	function formatDate(dateString: string): string {
		const date = new Date(dateString);
		return date.toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}

	function getExpiryStatus(expiresAt: string | null): { text: string; class: string } {
		if (!expiresAt) {
			return { text: 'Never', class: 'badge-success' };
		}

		const now = new Date();
		const expiry = new Date(expiresAt);

		if (expiry < now) {
			return { text: 'Expired', class: 'badge-error' };
		}

		const hoursUntilExpiry = (expiry.getTime() - now.getTime()) / (1000 * 60 * 60);

		if (hoursUntilExpiry < 24) {
			return { text: `${Math.round(hoursUntilExpiry)}h left`, class: 'badge-warning' };
		}

		const daysUntilExpiry = Math.round(hoursUntilExpiry / 24);
		return { text: `${daysUntilExpiry}d left`, class: 'badge-info' };
	}

	function formatAccessTime(dateString: string): string {
		return new Date(dateString).toLocaleString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

<div class="p-4 lg:p-6 max-w-7xl container mx-auto">
	<div class="space-y-4">
		<!-- Header -->
		<div class="flex items-center justify-between">
			<div>
				<h1 class="text-2xl lg:text-3xl font-bold">Shared Links</h1>
				<p class="text-base-content/70 mt-1">Manage public links created from files and folders</p>
			</div>
		</div>

		<div class="alert alert-info">
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
					d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"
				></path>
			</svg>
			<span>
				Public links can now be audited here. Create and edit links from the Share action in My
				Files, then use this page for revocation and access-log review.
			</span>
		</div>

		<!-- Shares List -->
		{#if $sharesQuery.isLoading}
			<div class="py-12 flex justify-center">
				<span class="loading loading-spinner loading-lg"></span>
			</div>
		{:else if $sharesQuery.isError}
			<div class="alert alert-error">
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
						d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
					></path>
				</svg>
				<span>Failed to load shares: {$sharesQuery.error?.message}</span>
			</div>
		{:else if $sharesQuery.data && $sharesQuery.data.length === 0}
			<!-- Empty State -->
			<div class="py-16 flex flex-col items-center justify-center text-center">
				<svg
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
					stroke-width="1.5"
					stroke="currentColor"
					class="w-20 h-20 lg:w-24 lg:h-24 text-base-content/20 mb-4"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						d="M7.217 10.907a2.25 2.25 0 100 2.186m0-2.186c.18.324.283.696.283 1.093s-.103.77-.283 1.093m0-2.186l9.566-5.314m-9.566 7.5l9.566 5.314m0 0a2.25 2.25 0 103.935 2.186 2.25 2.25 0 00-3.935-2.186zm0-12.814a2.25 2.25 0 103.933-2.185 2.25 2.25 0 00-3.933 2.185z"
					/>
				</svg>
				<h3 class="text-lg font-semibold mb-2">No shared links yet</h3>
				<p class="text-base-content/70 mb-4">
					Create and manage share links from the Share action on any file or folder in My Files
				</p>
				<a href="/files" class="btn btn-primary"> Go to My Files </a>
			</div>
		{:else if $sharesQuery.data}
			<!-- Shares Table -->
			<div class="bg-base-100 rounded-lg shadow overflow-x-auto">
				<table class="table-zebra table">
					<thead>
						<tr>
							<th>Resource</th>
							<th>Created</th>
							<th>Expires</th>
							<th>Status</th>
							<th class="text-right">Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each $sharesQuery.data as share}
							{@const expiryStatus = getExpiryStatus(share.expires_at)}
							<tr class="hover">
								<td>
									<div class="gap-3 flex items-center">
										<span class="text-2xl">{share.resource_type === 'folder' ? '📁' : '📄'}</span>
										<div>
											<div class="font-medium">{share.resource_name || 'Unknown Resource'}</div>
											<div class="text-xs text-base-content/60 gap-2 flex">
												<span class="badge badge-xs badge-ghost">
													{share.resource_type === 'folder' ? 'Folder' : 'File'}
												</span>
												{#if share.upload_only}
													<span class="badge badge-xs badge-warning">Upload Only</span>
												{/if}
												{#if share.password_protected}
													<span class="badge badge-xs badge-ghost">🔒 Password</span>
												{/if}
												<span class="badge badge-xs badge-ghost">{share.permissions}</span>
												<span class="badge badge-xs badge-ghost">
													{share.access_count} access{share.access_count === 1 ? '' : 'es'}
												</span>
											</div>
										</div>
									</div>
								</td>
								<td>{formatDate(share.created_at)}</td>
								<td>
									<span class="badge {expiryStatus.class}">
										{expiryStatus.text}
									</span>
								</td>
								<td>
									{#if expiryStatus.text === 'Expired'}
										<span class="badge badge-ghost">Inactive</span>
									{:else}
										<span class="badge badge-success">Active</span>
									{/if}
								</td>
								<td class="text-right">
									<div class="dropdown dropdown-end">
										<button
											type="button"
											class="btn btn-ghost btn-xs"
											aria-label="Open share actions"
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
													d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"
												/>
											</svg>
										</button>
										<ul class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-52 z-[1]">
											<li>
												<button type="button" on:click={() => copyShareLink(share.share_token)}>
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
															d="M8.25 7.5V6.108c0-1.135.845-2.098 1.976-2.192.373-.03.748-.057 1.123-.08M15.75 18H18a2.25 2.25 0 002.25-2.25V6.108c0-1.135-.845-2.098-1.976-2.192a48.424 48.424 0 00-1.123-.08M15.75 18.75v-1.875a3.375 3.375 0 00-3.375-3.375h-1.5a1.125 1.125 0 01-1.125-1.125v-1.5A3.375 3.375 0 006.375 7.5H5.25m11.9-3.664A2.251 2.251 0 0015 2.25h-1.5a2.251 2.251 0 00-2.15 1.586m5.8 0c.065.21.1.433.1.664v.75h-6V4.5c0-.231.035-.454.1-.664M6.75 7.5H4.875c-.621 0-1.125.504-1.125 1.125v12c0 .621.504 1.125 1.125 1.125h9.75c.621 0 1.125-.504 1.125-1.125V16.5a9 9 0 00-9-9z"
														/>
													</svg>
													Copy Link
												</button>
											</li>
											<li>
												<button type="button" on:click={() => toggleShareActivity(share)}>
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
															d="M3 3v18h18M7.5 15l3-3 2.25 2.25L16.5 9"
														/>
													</svg>
													{activeShareActivityId === share.id ? 'Hide Activity' : 'View Activity'}
												</button>
											</li>
											<li>
												<button
													type="button"
													on:click={() => handleRevokeShare(share)}
													class="text-error"
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
															d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0"
														/>
													</svg>
													Revoke
												</button>
											</li>
										</ul>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			{#if activeShareActivityId}
				<div class="bg-base-100 rounded-lg shadow p-4">
					<div class="mb-3 flex items-center justify-between">
						<div>
							<h2 class="text-lg font-semibold">Share Activity</h2>
							<p class="text-sm text-base-content/70">
								Recent access attempts for the selected public link
							</p>
						</div>
						<button type="button" class="btn btn-ghost btn-sm" on:click={closeShareActivity}>
							Close
						</button>
					</div>

					{#if shareActivityLoading}
						<div class="py-8 flex justify-center">
							<span class="loading loading-spinner loading-md"></span>
						</div>
					{:else if shareActivityError}
						<div class="alert alert-error">
							<span>{shareActivityError}</span>
						</div>
					{:else if shareActivity.length === 0}
						<div class="text-sm text-base-content/70 py-4">
							No recorded access yet for this share.
						</div>
					{:else}
						<div class="overflow-x-auto">
							<table class="table-sm table">
								<thead>
									<tr>
										<th>Time</th>
										<th>Action</th>
										<th>Actor</th>
										<th>IP</th>
										<th>Status</th>
									</tr>
								</thead>
								<tbody>
									{#each shareActivity as entry}
										<tr>
											<td>{formatAccessTime(entry.accessed_at)}</td>
											<td class="text-xs uppercase">{entry.action}</td>
											<td>{entry.actor_label || entry.actor_type || 'Anonymous'}</td>
											<td class="font-mono text-xs">{entry.ip_address || 'Unknown'}</td>
											<td>
												<span class="badge {entry.success ? 'badge-success' : 'badge-error'}">
													{entry.success ? 'Success' : 'Failed'}
												</span>
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{/if}
				</div>
			{/if}
		{/if}
	</div>
</div>

<!-- Toast Notifications -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
