<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listAllUserShares, revokeShare } from '$lib/api/shares';
	import { getShareTypeLabel } from '$lib/api/types';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { FileText, Folder, Plus, Link, Copy, Trash2, Clock } from 'lucide-svelte';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { toastStore } from '$lib/stores/toast';
	import { queryClient } from '$lib/query-client';

	let { module }: { module: ModuleDefinition } = $props();

	const sharesQuery = createQuery({
		queryKey: ['user-shares-module'],
		queryFn: () => listAllUserShares()
	});

	let shares = $derived($sharesQuery.data ?? []);

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
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function handleNewShare() {
		goto('/files');
	}
</script>

<ModulePageShell title="Shares" subtitle="Manage items shared from your workspace.">
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
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if shares.length === 0}
			<EmptyState
				icon={'🔗'}
				title={module.ui.page.emptyStateTitle}
				description={module.ui.page.emptyStateDescription}
				actionLabel={module.ui.page.primaryAction?.label}
				onAction={handleNewShare}
			/>
		{:else}
			<div class="flex flex-col gap-3">
				{#each shares as share}
					{@const href =
						share.resource_type === 'file'
							? `/files?file=${share.resource_id}`
							: `/files?folder=${share.resource_id}`}
					<div
						class="flex items-center gap-3 rounded-xl border border-base-300/50 bg-base-100 p-4 shadow-sm transition-all hover:border-brand-500/40"
					>
						<a href={href} class="flex min-w-0 flex-1 items-center gap-3">
							{#if share.resource_type === 'file'}
								<FileText size={18} class="shrink-0 text-brand-500" />
							{:else}
								<Folder size={18} class="shrink-0 text-brand-500" />
							{/if}
							<div class="flex min-w-0 flex-1 flex-col gap-1">
								<div class="flex flex-wrap items-center gap-2">
									<span class="truncate text-sm font-medium text-base-content">
										{share.resource_name || 'Untitled'}
									</span>
									<span class="badge badge-sm">{share.permissions}</span>
									<span class="badge badge-sm badge-info">{getShareTypeLabel(share)}</span>
								</div>
								<div class="flex items-center gap-1 text-xs text-base-content/50">
									<Clock size={12} />
									<span>{new Date(share.created_at).toLocaleDateString()}</span>
								</div>
							</div>
						</a>
						<div class="flex shrink-0 items-center gap-2">
							{#if share.share_token}
								<button
									type="button"
									class="btn btn-ghost btn-xs gap-1"
									onclick={() => handleCopyLink(share.share_token!)}
									title="Copy link"
								>
									<Copy size={14} />
								</button>
							{/if}
							<button
								type="button"
								class="btn btn-ghost btn-xs gap-1 text-error"
								onclick={() => handleRevoke(share.id)}
								title="Revoke share"
							>
									<Trash2 size={14} />
								</button>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	</ModulePageShell>
