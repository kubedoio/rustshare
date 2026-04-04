<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { goto } from '$app/navigation';
	import { listReceivedShares, listAllUserShares, revokeShare } from '$lib/api/shares';
	import { sharedResourcePath } from '$lib/utils/shared';
	import { formatDate, formatFileSize } from '$lib/utils/format';
	import { queryClient } from '$lib/query-client';
	import Toast from '$lib/components/common/Toast.svelte';
	import {
		Users,
		UserPlus,
		Link2,
		FolderOpen,
		FileText,
		ExternalLink,
		Trash2,
		Copy,
		Globe,
		Lock,
		Shield,
		Search,
		Calendar,
		ArrowRight
	} from 'lucide-svelte';

	let activeTab: 'received' | 'created' = 'received';
	let searchQuery = '';
	let showToast = false;
	let toastMessage = '';
	let toastType: 'success' | 'error' | 'info' = 'info';

	// Queries
	const receivedSharesQuery = createQuery({
		queryKey: ['received-shares'],
		queryFn: listReceivedShares
	});

	const createdSharesQuery = createQuery({
		queryKey: ['user-shares'],
		queryFn: listAllUserShares
	});

	// Mutations
	const revokeMutation = createMutation({
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
		setTimeout(() => (showToast = false), 3000);
	}

	function permissionLabel(permission: string): string {
		if (permission === 'Admin') return 'Manage';
		if (permission === 'Edit') return 'Edit';
		return 'View';
	}

	function openSharedResource(resourceType: string, resourceId: string) {
		goto(sharedResourcePath(resourceType as any, resourceId));
	}

	function handleRevoke(id: string, name: string) {
		if (confirm(`Are you sure you want to revoke access to "${name}"?`)) {
			$revokeMutation.mutate(id);
		}
	}

	function copyLink(token: string | null) {
		if (!token) return;
		const url = `${window.location.origin}/share/${token}`;
		navigator.clipboard.writeText(url);
		displayToast('Link copied to clipboard', 'success');
	}

	function isExpired(expiresAt: string | null): boolean {
		return expiresAt ? new Date(expiresAt) < new Date() : false;
	}

	$: receivedShares = $receivedSharesQuery.data || [];
	$: createdShares = $createdSharesQuery.data || [];

	$: filteredReceived = receivedShares.filter(s => 
		s.resource_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
		s.shared_by_name.toLowerCase().includes(searchQuery.toLowerCase())
	);

	$: filteredCreated = createdShares.filter(s => 
		(s.resource_name ?? '').toLowerCase().includes(searchQuery.toLowerCase())
	);
</script>

<svelte:head>
	<title>Shared - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-6xl space-y-8 p-4 lg:p-8">
	<!-- Hero Header -->
	<header class="relative overflow-hidden rounded-[1.5rem] border border-base-300/70 bg-gradient-to-br from-base-100 via-base-100 to-base-200/80 shadow-panel p-6 lg:p-8">
		<div class="relative z-10 max-w-2xl">
			<div class="rs-kicker mb-4 inline-flex items-center gap-2 rounded-full bg-brand-500/10 px-3 py-1 text-xs font-bold uppercase tracking-wider text-brand-600 border border-brand-500/20">
				<Users class="h-3.5 w-3.5" />
				Collaboration Hub
			</div>
			<h1 class="font-display text-2xl leading-[1.2] tracking-tight text-base-content lg:text-3xl">
				Manage access with <span class="text-brand-500">absolute</span> clarity
			</h1>
			<p class="mt-4 text-sm leading-relaxed text-base-content/60 lg:text-base">
				Whether it's files shared with you or links you've sent out, track every permission and revoke access in a single click.
			</p>
		</div>

		<!-- Decorative elements -->
		<div class="absolute -right-10 -top-10 h-32 w-32 rounded-full bg-brand-500/5 blur-3xl"></div>
		<div class="absolute -bottom-10 left-1/2 h-48 w-48 rounded-full bg-brand-500/5 blur-3xl"></div>
	</header>

	<!-- Main Content Section -->
	<div class="space-y-6">
		<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
			<!-- Tab Switcher -->
			<div class="inline-flex rounded-2xl bg-base-200/50 p-1 border border-base-300/50">
				<button 
					class="flex items-center gap-2 rounded-[0.875rem] px-6 py-2.5 text-sm font-semibold transition-all duration-200
						{activeTab === 'received' ? 'bg-white text-base-content shadow-sm' : 'text-base-content/50 hover:text-base-content/80'}"
					on:click={() => activeTab = 'received'}
				>
					<Users class="h-4 w-4" />
					Shared with me
					{#if receivedShares.length > 0}
						<span class="ml-1 rounded-full bg-base-300/50 px-2 py-0.5 text-[10px]">{receivedShares.length}</span>
					{/if}
				</button>
				<button 
					class="flex items-center gap-2 rounded-[0.875rem] px-6 py-2.5 text-sm font-semibold transition-all duration-200
						{activeTab === 'created' ? 'bg-white text-base-content shadow-sm' : 'text-base-content/50 hover:text-base-content/80'}"
					on:click={() => activeTab = 'created'}
				>
					<Link2 class="h-4 w-4" />
					Created by me
					{#if createdShares.length > 0}
						<span class="ml-1 rounded-full bg-base-300/50 px-2 py-0.5 text-[10px]">{createdShares.length}</span>
					{/if}
				</button>
			</div>

			<!-- Search -->
			<div class="relative max-w-sm flex-1">
				<Search class="absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-base-content/30" />
				<input 
					type="text" 
					placeholder="Filter by name or user..." 
					bind:value={searchQuery}
					class="w-full rounded-2xl border-base-300/70 bg-base-100 py-2.5 pl-11 pr-4 text-sm transition-all focus:border-brand-500/40 focus:ring-4 focus:ring-brand-500/5"
				/>
			</div>
		</div>

		{#if activeTab === 'received'}
			<!-- Shared With Me View -->
			{#if $receivedSharesQuery.isLoading}
				<div class="flex h-64 items-center justify-center">
					<span class="loading loading-spinner loading-lg text-brand-500"></span>
				</div>
			{:else if filteredReceived.length === 0}
				<div class="flex flex-col items-center justify-center rounded-2xl border border-dashed border-base-300 bg-base-100/50 py-16 text-center">
					<div class="mb-4 rounded-2xl bg-brand-500/5 p-4 text-brand-500">
						<Users class="h-8 w-8 opacity-50" />
					</div>
					<h3 class="font-display text-lg text-base-content">Nothing shared with you yet</h3>
					<p class="mt-2 max-w-xs text-sm text-base-content/50">Items shared directly with you by other users will appear here.</p>
				</div>
			{:else}
				<div class="grid gap-4">
					{#each filteredReceived as share}
						<div class="group relative rounded-xl border border-base-300/70 bg-base-100 p-4 transition-all duration-300 hover:border-brand-500/20 hover:shadow-panel hover:shadow-brand-500/5">
							<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
								<div class="flex items-center gap-4">
									<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-base-200/50 text-brand-500 transition-colors group-hover:bg-brand-500/10">
										{#if share.resource_type === 'folder'}
											<FolderOpen class="h-5 w-5" />
										{:else}
											<FileText class="h-5 w-5" />
										{/if}
									</div>
									<div class="min-w-0">
										<h3 class="truncate font-display text-base font-semibold text-base-content group-hover:text-brand-600 transition-colors">
											{share.resource_name}
										</h3>
										<div class="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-base-content/50">
											<span class="flex items-center gap-1">
												<Users class="h-3 w-3" />
												By {share.shared_by_name}
											</span>
											<span class="flex items-center gap-1">
												<Calendar class="h-3 w-3" />
												{formatDate(share.created_at)}
											</span>
											<span class="flex items-center gap-1 rounded-full bg-base-200 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-base-content/70">
												{permissionLabel(share.permission)}
											</span>
										</div>
									</div>
								</div>
								<div class="flex shrink-0 items-center gap-3">
									<button 
										class="flex h-9 items-center gap-2 rounded-lg bg-brand-500 px-4 text-sm font-semibold text-white shadow-md shadow-brand-500/20 transition-all hover:bg-brand-600"
										on:click={() => openSharedResource(share.resource_type, share.resource_id)}
									>
										Open
										<ArrowRight class="h-4 w-4" />
									</button>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		{:else}
			<!-- Created By Me View -->
			{#if $createdSharesQuery.isLoading}
				<div class="flex h-64 items-center justify-center">
					<span class="loading loading-spinner loading-lg text-brand-500"></span>
				</div>
			{:else if filteredCreated.length === 0}
				<div class="flex flex-col items-center justify-center rounded-2xl border border-dashed border-base-300 bg-base-100/50 py-16 text-center">
					<div class="mb-4 rounded-2xl bg-brand-500/5 p-4 text-brand-500">
						<Link2 class="h-8 w-8 opacity-50" />
					</div>
					<h3 class="font-display text-lg text-base-content">No active shares found</h3>
					<p class="mt-2 max-w-xs text-sm text-base-content/50">Share your files or folders to see them listed and managed here.</p>
					<button 
						class="mt-4 flex items-center gap-2 rounded-lg bg-brand-500 px-4 py-2 text-sm font-semibold text-white shadow-md shadow-brand-500/20 hover:bg-brand-600 transition-all"
						on:click={() => goto('/files')}
					>
						Browse Files
					</button>
				</div>
			{:else}
				<div class="grid gap-4">
					{#each filteredCreated as share}
						<div class="group relative rounded-xl border border-base-300/70 bg-base-100 p-4 transition-all duration-300 hover:border-brand-500/20 hover:shadow-panel hover:shadow-brand-500/5">
							<div class="flex flex-col gap-4 lg:flex-row lg:items-center">
								<div class="flex flex-1 items-center gap-4">
									<div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-base-200/50 text-brand-500 transition-colors group-hover:bg-brand-500/10">
										{#if share.resource_type === 'folder'}
											<FolderOpen class="h-5 w-5" />
										{:else}
											<FileText class="h-5 w-5" />
										{/if}
									</div>
									<div class="min-w-0 flex-1">
										<div class="flex items-center gap-3">
											<h3 class="truncate font-display text-base font-semibold text-base-content group-hover:text-brand-600 transition-colors">
												{share.resource_name}
											</h3>
											{#if isExpired(share.expires_at)}
												<span class="rounded-full bg-error/10 px-2.5 py-0.5 text-[10px] font-bold uppercase text-error border border-error/10">Expired</span>
											{:else}
												<span class="rounded-full bg-success/10 px-2.5 py-0.5 text-[10px] font-bold uppercase text-success border border-success/10">Active</span>
											{/if}
										</div>
										<div class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-base-content/50 font-data">
											<span class="flex items-center gap-1">
												<Globe class="h-3 w-3" />
												{share.access_count} visits
											</span>
											<span class="flex items-center gap-1">
												<Shield class="h-3 w-3" />
												{share.permissions} Access
											</span>
											{#if share.share_token}
												<span class="flex items-center gap-1 text-brand-500 font-medium">
													<Globe class="h-3 w-3" />
													Public Link
												</span>
											{:else if share.recipient_user_id || share.recipient_group_id}
												<span class="flex items-center gap-1 text-info font-medium">
													<Users class="h-3 w-3" />
													Direct Share
												</span>
											{/if}
										</div>
									</div>
								</div>
								
								<div class="flex shrink-0 flex-wrap items-center gap-2">
									{#if share.share_token}
										<button 
											class="flex h-10 items-center gap-2 rounded-xl bg-base-200/50 px-4 text-sm font-semibold text-base-content/70 transition-all hover:bg-base-200 hover:text-base-content border border-base-300/50"
											on:click={() => copyLink(share.share_token)}
											title="Copy share link"
										>
											<Copy class="h-4 w-4" />
											Copy
										</button>
									{/if}
									<button 
										class="flex h-10 items-center justify-center rounded-xl border border-error/20 bg-error/5 px-4 text-sm font-semibold text-error transition-all hover:bg-error/10"
										on:click={() => handleRevoke(share.id, share.resource_name ?? 'Resource')}
									>
										<Trash2 class="h-4 w-4 mr-2" />
										Revoke
									</button>
								</div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</div>

{#if showToast}
	<Toast message={toastMessage} type={toastType} onClose={() => showToast = false} />
{/if}

<style>
	.rs-kicker {
		font-family: var(--font-data);
	}
	
	/* Smooth transitions for the cards */
	.group {
		will-change: transform, box-shadow;
	}
</style>
