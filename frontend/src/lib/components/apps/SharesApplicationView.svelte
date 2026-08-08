<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listAllUserShares, revokeShare } from '$lib/api/shares';
	import { getShareTypeLabel } from '$lib/api/types';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ApplicationPageSkeleton from '$lib/components/common/ApplicationPageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ApplicationPageShell from '$lib/components/layout/ApplicationPageShell.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import { formatAbsoluteDate } from '$lib/utils/format';
	import {
		FileText,
		Folder,
		Plus,
		Link,
		Copy,
		Trash2,
		Clock,
		MoreHorizontal,
		X,
		Lock,
		Users
	} from 'lucide-svelte';
	import { resolveApplicationFolderId } from '$lib/applications/applicationPages';
	import type { ApplicationDefinition } from '$lib/applications/registry';
	import { toastStore } from '$lib/stores/toast';
	import { queryClient } from '$lib/query-client';

	let { module }: { module: ApplicationDefinition } = $props();
	let selectedShareId = $state<string | null>(null);
	let showNewShareModal = $state(false);
	let shareFilter = $state<'all' | 'internal' | 'links'>('all');
	let sortDirection = $state<'desc' | 'asc'>('desc');

	const sharesQuery = createQuery({
		queryKey: ['user-shares-module'],
		queryFn: () => listAllUserShares()
	});

	let shares = $derived($sharesQuery.data ?? []);
	let filteredShares = $derived(
		shares
			.filter((share) => {
				if (shareFilter === 'links') return !!share.share_token;
				if (shareFilter === 'internal') return !share.share_token;
				return true;
			})
			.toSorted((a, b) => {
				const aTime = new Date(a.created_at).getTime();
				const bTime = new Date(b.created_at).getTime();
				return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
			})
	);
	let selectedShare = $derived(
		selectedShareId ? (shares.find((share) => share.id === selectedShareId) ?? null) : null
	);
	let linkShareCount = $derived(shares.filter((share) => share.share_token).length);
	let internalShareCount = $derived(shares.filter((share) => !share.share_token).length);

	async function handleCopyLink(token: string) {
		const url = `${window.location.origin}/share/${token}`;
		await navigator.clipboard.writeText(url);
		toastStore.show('Link copied', 'success');
	}

	async function handleRevoke(shareId: string) {
		try {
			await revokeShare(shareId);
			queryClient.invalidateQueries({ queryKey: ['user-shares-module'] });
			toastStore.show('Share revoked', 'success');
		} catch (err) {
			console.error('Failed to revoke share:', err);
			toastStore.show('Failed to revoke share', 'error');
		}
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveApplicationFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function handleNewShare() {
		showNewShareModal = true;
	}

	function handleBrowseFiles() {
		showNewShareModal = false;
		goto('/files');
	}

	function shareAccessSummary(share: (typeof shares)[number]) {
		if (share.share_token) {
			return share.password_protected ? 'Link with password' : 'Link';
		}
		return getShareTypeLabel(share);
	}
</script>

<ApplicationPageShell title="Shares" subtitle="Manage items shared from your workspace.">
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleNewShare}>
			<Plus size={14} />
			<span>New share</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if $sharesQuery.isLoading}
			<ApplicationPageSkeleton />
		{:else if $sharesQuery.isError}
			<ErrorState
				title="Failed to load shares"
				message={$sharesQuery.error?.message || 'Unknown error'}
				onRetry={() => $sharesQuery.refetch()}
			/>
		{:else if shares.length === 0}
			<EmptyState
				icon={'🔗'}
				title={module.ui.page.emptyStateTitle}
				description={module.ui.page.emptyStateDescription}
				actionLabel={module.ui.page.primaryAction?.label}
				onAction={handleNewShare}
			/>
		{:else}
			<div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
				<div class="flex flex-col gap-4">
					<div class="flex flex-wrap items-center justify-between gap-3">
						<div class="flex flex-wrap gap-2">
							<button
								class="btn rounded-full btn-sm {shareFilter === 'all'
									? 'text-brand-600 btn-outline'
									: 'btn-ghost'}"
								onclick={() => (shareFilter = 'all')}
							>
								All shares <span class="badge badge-sm">{shares.length}</span>
							</button>
							<button
								class="btn rounded-full btn-sm {shareFilter === 'internal'
									? 'text-brand-600 btn-outline'
									: 'btn-ghost'}"
								onclick={() => (shareFilter = shareFilter === 'internal' ? 'all' : 'internal')}
							>
								Internal <span class="badge badge-sm">{internalShareCount}</span>
							</button>
							<button
								class="btn rounded-full btn-sm {shareFilter === 'links'
									? 'text-brand-600 btn-outline'
									: 'btn-ghost'}"
								onclick={() => (shareFilter = shareFilter === 'links' ? 'all' : 'links')}
							>
								Links <span class="badge badge-sm">{linkShareCount}</span>
							</button>
						</div>
						<button
							class="btn btn-ghost btn-sm"
							onclick={() => (sortDirection = sortDirection === 'desc' ? 'asc' : 'desc')}
						>
							Sort by: {sortDirection === 'desc' ? 'Newest' : 'Oldest'}
						</button>
					</div>
					<div class="flex flex-col gap-3">
						{#each filteredShares as share}
							<button
								type="button"
								class="flex items-center gap-4 rounded-xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 {selectedShare?.id ===
								share.id
									? 'border-brand-500/35'
									: ''}"
								onclick={() => (selectedShareId = share.id)}
							>
								<div
									class="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
								>
									{#if share.resource_type === 'file'}
										<FileText size={22} />
									{:else}
										<Folder size={22} />
									{/if}
								</div>
								<div class="flex min-w-0 flex-1 flex-col gap-1">
									<span class="truncate text-sm font-semibold text-base-content">
										{share.resource_name || 'Untitled'}
									</span>
									<span class="text-xs text-base-content/55">
										{share.resource_type === 'file' ? 'File' : 'Folder'} • {shareAccessSummary(
											share
										)} • {share.permissions}
									</span>
									{#if share.share_token}
										<span class="inline-flex items-center gap-1 truncate text-xs text-brand-500">
											<Link size={12} />
											/share/{share.share_token}
										</span>
									{:else}
										<span class="inline-flex items-center gap-1 text-xs text-base-content/55">
											<Users size={12} />
											Specific access
										</span>
									{/if}
								</div>
								<span class="hidden text-xs text-base-content/55 md:block">
									{formatAbsoluteDate(share.created_at)}
								</span>
								{#if share.share_token}
									<span class="badge badge-sm badge-success">Active</span>
								{:else}
									<span class="badge badge-ghost badge-sm">{getShareTypeLabel(share)}</span>
								{/if}
								<MoreHorizontal size={16} class="text-base-content/45" />
							</button>
						{/each}
					</div>
				</div>

				{#if selectedShare}
					<aside class="rounded-xl border border-base-300/70 bg-base-100 shadow-sm">
						<div class="flex items-start justify-between border-b border-base-200 p-4">
							<div>
								<h3 class="font-semibold text-base-content">Share details</h3>
							</div>
							<button
								class="btn btn-square btn-ghost btn-sm"
								onclick={() => (selectedShareId = null)}
							>
								<X size={16} />
							</button>
						</div>
						<div class="flex items-center gap-3 border-b border-base-200 p-4">
							<div
								class="flex h-12 w-12 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
							>
								{#if selectedShare.resource_type === 'file'}
									<FileText size={22} />
								{:else}
									<Folder size={22} />
								{/if}
							</div>
							<div>
								<p class="font-semibold">{selectedShare.resource_name || 'Untitled'}</p>
								<p class="text-xs text-base-content/55">
									{selectedShare.resource_type === 'file' ? 'File' : 'Folder'} • {selectedShare.share_token
										? 'Link'
										: 'Internal only'}
								</p>
								<p class="text-xs text-base-content/45">
									Created {formatAbsoluteDate(selectedShare.created_at)}
								</p>
							</div>
						</div>
						<div class="space-y-5 p-4">
							<section>
								<h4 class="mb-2 text-sm font-semibold">Access</h4>
								<div class="flex items-center gap-2 text-sm">
									<Lock size={16} class="text-base-content/50" />
									<span>{selectedShare.share_token ? 'Link (view only)' : 'Internal only'}</span>
								</div>
								<p class="mt-1 text-xs text-base-content/50">
									{selectedShare.share_token
										? 'Anyone with the link can view.'
										: 'Only people in your workspace can access this.'}
								</p>
							</section>
							<section>
								<h4 class="mb-2 text-sm font-semibold">Access details</h4>
								<div class="rounded-lg border border-base-200 p-3 text-sm">
									<div class="flex items-center justify-between">
										<span>Permission</span>
										<span class="font-medium">{selectedShare.permissions}</span>
									</div>
									<div class="mt-2 flex items-center justify-between">
										<span>Visits</span>
										<span class="font-medium">{selectedShare.access_count ?? 0}</span>
									</div>
									{#if selectedShare.expires_at}
										<div class="mt-2 flex items-center justify-between">
											<span>Expires</span>
											<span class="font-medium">{formatAbsoluteDate(selectedShare.expires_at)}</span
											>
										</div>
									{/if}
								</div>
							</section>
							<section>
								<h4 class="mb-2 text-sm font-semibold">Activity</h4>
								<p class="flex items-center gap-2 text-xs text-base-content/55">
									<Clock size={14} />
									Created {formatAbsoluteDate(selectedShare.created_at)}
								</p>
							</section>
						</div>
						<div class="flex items-center justify-between border-t border-base-200 p-4">
							<button
								class="btn gap-2 btn-outline btn-sm"
								disabled={!selectedShare.share_token}
								onclick={() =>
									selectedShare.share_token && handleCopyLink(selectedShare.share_token)}
							>
								<Copy size={14} />
								Copy link
							</button>
							<button
								class="btn gap-2 btn-outline btn-sm btn-error"
								onclick={() => handleRevoke(selectedShare.id)}
							>
								<Trash2 size={14} />
								Revoke share
							</button>
						</div>
					</aside>
				{/if}
			</div>
		{/if}
	</div>
</ApplicationPageShell>

<ModalBase open={showNewShareModal} title="New share" onClose={() => (showNewShareModal = false)}>
	<div class="flex min-h-56 flex-col justify-between gap-6">
		<div class="flex flex-col items-center gap-3 py-6 text-center">
			<Folder size={42} class="text-brand-500" />
			<h3 class="text-base font-semibold">Choose a file or folder</h3>
			<p class="max-w-sm text-sm text-base-content/55">
				Shares are created from the Files view so the selected file or folder can be used as the
				source.
			</p>
		</div>
		<div class="flex justify-between">
			<button class="btn btn-ghost btn-sm" onclick={() => (showNewShareModal = false)}
				>Cancel</button
			>
			<button class="btn btn-sm btn-primary" onclick={handleBrowseFiles}> Open Files </button>
		</div>
	</div>
</ModalBase>
