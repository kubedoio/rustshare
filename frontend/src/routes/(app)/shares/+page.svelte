<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { getShareAccessLog, listAllUserShares, revokeShare } from '$lib/api/shares';
	import { queryClient } from '$lib/query-client';
	import type { Share, ShareAccessLogEntry } from '$lib/api/types';
	import Toast from '$lib/components/common/Toast.svelte';
	import {
		Activity,
		Clock3,
		Copy,
		FileText,
		FolderOpen,
		Globe,
		Link2,
		Lock,
		Shield,
		Trash2,
		Upload,
		Users
	} from 'lucide-svelte';

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

	function isExpired(expiresAt: string | null): boolean {
		return expiresAt ? new Date(expiresAt) < new Date() : false;
	}

	function isExpiringSoon(expiresAt: string | null): boolean {
		if (!expiresAt || isExpired(expiresAt)) return false;
		return new Date(expiresAt).getTime() - Date.now() < 1000 * 60 * 60 * 24 * 3;
	}

	function permissionBadgeClass(permission: Share['permissions']): string {
		switch (permission) {
			case 'Admin':
				return 'border-error/20 bg-error/10 text-error';
			case 'Edit':
				return 'border-warning/20 bg-warning/10 text-warning';
			default:
				return 'border-brand-500/20 bg-brand-500/10 text-brand-500';
		}
	}

	$: shares = $sharesQuery.data || [];
	$: activeShareCount = shares.filter((share) => !isExpired(share.expires_at)).length;
	$: expiringSoonCount = shares.filter((share) => isExpiringSoon(share.expires_at)).length;
	$: totalAccessCount = shares.reduce((sum, share) => sum + share.access_count, 0);
</script>

<div class="mx-auto max-w-6xl space-y-6 p-4 lg:p-6">
	<div class="space-y-6">
		<!-- Header -->
		<div class="overflow-hidden rounded-[2rem] border border-base-300/70 bg-gradient-to-br from-base-200 via-base-100 to-base-100">
			<div class="flex flex-col gap-6 p-6 lg:flex-row lg:items-end lg:justify-between lg:p-8">
				<div class="max-w-2xl">
					<div class="mb-4 inline-flex items-center gap-2 rounded-full border border-brand-500/20 bg-brand-500/10 px-3 py-1 text-xs font-semibold uppercase tracking-[0.18em] text-brand-500">
						<Link2 class="h-3.5 w-3.5" />
						Share Control Center
					</div>
					<h1 class="text-3xl font-semibold tracking-tight text-base-content lg:text-4xl">
						Shared links that feel managed, not forgotten
					</h1>
					<p class="mt-3 max-w-xl text-sm leading-6 text-base-content/70 lg:text-base">
						Review every public link, see what is still active, and revoke access before stale links turn into clutter.
					</p>
				</div>
				<a
					href="/files"
					class="inline-flex items-center justify-center rounded-2xl bg-brand-500 px-4 py-3 text-sm font-medium text-white shadow-sm shadow-brand-500/20 transition-colors hover:bg-brand-600"
				>
					Create from My Files
				</a>
			</div>
		</div>

		<div class="grid gap-4 md:grid-cols-3">
			<div class="rounded-3xl border border-base-300/70 bg-base-100 p-5 shadow-sm">
				<div class="flex items-start justify-between">
					<div>
						<p class="text-sm font-medium text-base-content/60">Active links</p>
						<p class="mt-2 text-3xl font-semibold text-base-content">{activeShareCount}</p>
					</div>
					<div class="rounded-2xl bg-brand-500/10 p-3 text-brand-500">
						<Globe class="h-5 w-5" />
					</div>
				</div>
				<p class="mt-3 text-sm text-base-content/60">Links that are still usable right now.</p>
			</div>
			<div class="rounded-3xl border border-base-300/70 bg-base-100 p-5 shadow-sm">
				<div class="flex items-start justify-between">
					<div>
						<p class="text-sm font-medium text-base-content/60">Expiring soon</p>
						<p class="mt-2 text-3xl font-semibold text-base-content">{expiringSoonCount}</p>
					</div>
					<div class="rounded-2xl bg-warning/10 p-3 text-warning">
						<Clock3 class="h-5 w-5" />
					</div>
				</div>
				<p class="mt-3 text-sm text-base-content/60">Links worth reviewing in the next few days.</p>
			</div>
			<div class="rounded-3xl border border-base-300/70 bg-base-100 p-5 shadow-sm">
				<div class="flex items-start justify-between">
					<div>
						<p class="text-sm font-medium text-base-content/60">Recorded visits</p>
						<p class="mt-2 text-3xl font-semibold text-base-content">{totalAccessCount}</p>
					</div>
					<div class="rounded-2xl bg-success/10 p-3 text-success">
						<Activity class="h-5 w-5" />
					</div>
				</div>
				<p class="mt-3 text-sm text-base-content/60">Total tracked opens and downloads across links.</p>
			</div>
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
			<div class="rounded-[2rem] border border-dashed border-base-300 bg-base-100 px-6 py-16 text-center">
				<div class="mx-auto mb-5 flex h-16 w-16 items-center justify-center rounded-3xl bg-brand-500/10 text-brand-500">
					<Link2 class="h-8 w-8" />
				</div>
				<h3 class="text-xl font-semibold text-base-content">No shared links yet</h3>
				<p class="mx-auto mt-3 max-w-md text-sm leading-6 text-base-content/65">
					Create a share from any file or folder in My Files, then come back here to review access, expiry, and link health in one place.
				</p>
				<a
					href="/files"
					class="mt-6 inline-flex items-center justify-center rounded-2xl bg-brand-500 px-4 py-3 text-sm font-medium text-white transition-colors hover:bg-brand-600"
				>
					Go to My Files
				</a>
			</div>
		{:else if $sharesQuery.data}
			<div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
				<div class="space-y-4">
					{#each $sharesQuery.data as share}
						{@const expiryStatus = getExpiryStatus(share.expires_at)}
						{@const shareUrl = getShareUrl(share.share_token)}
						<div class="rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm transition-colors hover:border-brand-500/15">
							<div class="flex flex-col gap-5">
								<div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
									<div class="flex items-start gap-4">
										<div class="rounded-2xl border border-base-300/70 bg-base-200/70 p-3 text-brand-500">
											{#if share.resource_type === 'folder'}
												<FolderOpen class="h-5 w-5" />
											{:else}
												<FileText class="h-5 w-5" />
											{/if}
										</div>
										<div class="min-w-0">
											<h2 class="truncate text-lg font-semibold text-base-content">
												{share.resource_name || 'Unknown resource'}
											</h2>
											<div class="mt-2 flex flex-wrap gap-2 text-xs font-medium">
												<span class="rounded-full border border-base-300 bg-base-200 px-2.5 py-1 text-base-content/70">
													{share.resource_type === 'folder' ? 'Folder' : 'File'}
												</span>
												<span class="rounded-full border px-2.5 py-1 {permissionBadgeClass(share.permissions)}">
													{share.permissions}
												</span>
												{#if share.upload_only}
													<span class="rounded-full border border-warning/20 bg-warning/10 px-2.5 py-1 text-warning">
														Upload only
													</span>
												{/if}
												{#if share.password_protected}
													<span class="rounded-full border border-base-300 bg-base-200 px-2.5 py-1 text-base-content/70">
														<Lock class="mr-1 inline h-3 w-3" />
														Password
													</span>
												{/if}
											</div>
										</div>
									</div>
									<div class="flex flex-wrap gap-2">
										<button
											type="button"
											class="inline-flex items-center gap-2 rounded-xl border border-base-300 bg-base-100 px-3 py-2 text-sm font-medium text-base-content/75 transition-colors hover:border-brand-500/20 hover:text-base-content"
											on:click={() => copyShareLink(share.share_token)}
										>
											<Copy class="h-4 w-4" />
											Copy link
										</button>
										<button
											type="button"
											class="inline-flex items-center gap-2 rounded-xl border border-base-300 bg-base-100 px-3 py-2 text-sm font-medium text-base-content/75 transition-colors hover:border-brand-500/20 hover:text-base-content"
											on:click={() => toggleShareActivity(share)}
										>
											<Activity class="h-4 w-4" />
											{activeShareActivityId === share.id ? 'Hide activity' : 'View activity'}
										</button>
										<button
											type="button"
											class="inline-flex items-center gap-2 rounded-xl border border-error/20 bg-error/5 px-3 py-2 text-sm font-medium text-error transition-colors hover:bg-error/10"
											on:click={() => handleRevokeShare(share)}
										>
											<Trash2 class="h-4 w-4" />
											Revoke
										</button>
									</div>
								</div>

								<div class="rounded-2xl border border-base-300/70 bg-base-200/45 px-4 py-3">
									<p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/45">Share URL</p>
									<p class="mt-2 truncate font-mono text-xs text-base-content/70">{shareUrl}</p>
								</div>

								<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
									<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
										<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/45">Created</p>
										<p class="mt-2 text-sm font-medium text-base-content">{formatDate(share.created_at)}</p>
									</div>
									<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
										<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/45">Expiry</p>
										<p class="mt-2 text-sm font-medium text-base-content">{expiryStatus.text}</p>
									</div>
									<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
										<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/45">Access</p>
										<p class="mt-2 text-sm font-medium text-base-content">
											{share.access_count} visit{share.access_count === 1 ? '' : 's'}
										</p>
									</div>
									<div class="rounded-2xl border border-base-300/70 bg-base-100 px-4 py-3">
										<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/45">Status</p>
										<p class="mt-2 text-sm font-medium {isExpired(share.expires_at) ? 'text-error' : 'text-success'}">
											{isExpired(share.expires_at) ? 'Inactive' : 'Active'}
										</p>
									</div>
								</div>
							</div>
						</div>
					{/each}
				</div>

				<div class="space-y-4">
					<div class="rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm">
						<h2 class="text-lg font-semibold text-base-content">What to watch</h2>
						<div class="mt-4 space-y-3 text-sm text-base-content/70">
							<div class="flex items-start gap-3">
								<div class="rounded-xl bg-brand-500/10 p-2 text-brand-500">
									<Users class="h-4 w-4" />
								</div>
								<p>Use access counts to find links people still depend on before revoking them.</p>
							</div>
							<div class="flex items-start gap-3">
								<div class="rounded-xl bg-warning/10 p-2 text-warning">
									<Clock3 class="h-4 w-4" />
								</div>
								<p>Expiring links are fine, silent expired links are not. Review them before they surprise people.</p>
							</div>
							<div class="flex items-start gap-3">
								<div class="rounded-xl bg-base-200 p-2 text-base-content/70">
									<Shield class="h-4 w-4" />
								</div>
								<p>Password-protected and upload-only links stand out more clearly now, which is the point.</p>
							</div>
						</div>
					</div>

					{#if activeShareActivityId}
						<div class="rounded-[1.75rem] border border-base-300/70 bg-base-100 p-5 shadow-sm">
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
								<div class="rounded-2xl border border-base-300/70 bg-base-200/45 px-4 py-4 text-sm text-base-content/70">
									No recorded access yet for this share.
								</div>
							{:else}
								<div class="space-y-3">
									{#each shareActivity as entry}
										<div class="rounded-2xl border border-base-300/70 bg-base-200/35 px-4 py-3">
											<div class="flex items-start justify-between gap-3">
												<div>
													<p class="text-sm font-medium text-base-content">
														{entry.actor_label || entry.actor_type || 'Anonymous'}
													</p>
													<p class="mt-1 text-xs uppercase tracking-[0.16em] text-base-content/45">{entry.action}</p>
												</div>
												<span class="rounded-full px-2.5 py-1 text-xs font-medium {entry.success ? 'bg-success/10 text-success' : 'bg-error/10 text-error'}">
													{entry.success ? 'Success' : 'Failed'}
												</span>
											</div>
											<div class="mt-3 space-y-1 text-xs text-base-content/60">
												<p>{formatAccessTime(entry.accessed_at)}</p>
												<p class="font-mono">{entry.ip_address || 'Unknown IP'}</p>
											</div>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

<!-- Toast Notifications -->
{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => (showToast = false)} />
{/if}
