<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { goto } from '$app/navigation';
	import { listReceivedShares } from '$lib/api/shares';
	import { sharedResourcePath } from '$lib/utils/shared';
	import { formatDate } from '$lib/utils/format';

	const receivedSharesQuery = createQuery({
		queryKey: ['received-shares'],
		queryFn: listReceivedShares
	});

	function permissionLabel(permission: 'View' | 'Edit' | 'Admin'): string {
		if (permission === 'Admin') return 'Manage';
		if (permission === 'Edit') return 'Edit';
		return 'View';
	}

	function resourceIcon(resourceType: 'file' | 'folder'): string {
		return resourceType === 'folder' ? '📁' : '📄';
	}

	function openShare(resourceType: 'file' | 'folder', resourceId: string) {
		goto(sharedResourcePath(resourceType, resourceId));
	}
</script>

<svelte:head>
	<title>Shared with Me - RustShare</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center justify-between gap-4">
		<div>
			<h1 class="text-3xl font-bold">Shared with Me</h1>
			<p class="text-base-content/70 mt-1">
				Files and folders other users have shared directly with you
			</p>
		</div>
		<button class="btn btn-outline" on:click={() => goto('/files')}>Go to My Files</button>
	</div>

	{#if $receivedSharesQuery.isLoading}
		<div class="py-12 flex justify-center">
			<span class="loading loading-spinner loading-lg"></span>
		</div>
	{:else if $receivedSharesQuery.isError}
		<div class="alert alert-error">
			<span>Failed to load received shares: {$receivedSharesQuery.error?.message}</span>
		</div>
	{:else if $receivedSharesQuery.data && $receivedSharesQuery.data.length === 0}
		<div class="card bg-base-100 shadow-xl">
			<div class="card-body flex flex-col items-center justify-center py-16">
				<div class="text-6xl mb-4">👥</div>
				<h2 class="text-2xl font-bold mb-2">Nothing shared with you yet</h2>
				<p class="text-base-content/70 mb-6 text-center max-w-md">
					When another user shares a file or folder with you, it will appear here with its
					permission level and who shared it.
				</p>
				<button class="btn btn-primary" on:click={() => goto('/files')}>Browse My Files</button>
			</div>
		</div>
	{:else if $receivedSharesQuery.data}
		<div class="bg-base-100 rounded-lg shadow overflow-x-auto">
			<table class="table table-zebra">
				<thead>
					<tr>
						<th>Resource</th>
						<th>Shared By</th>
						<th>Permission</th>
						<th>Shared</th>
					</tr>
				</thead>
				<tbody>
					{#each $receivedSharesQuery.data as share}
						<tr class="hover">
							<td>
								<div class="flex items-center gap-3">
									<span class="text-2xl">{resourceIcon(share.resource_type)}</span>
									<div>
										<button
											type="button"
											class="font-medium text-left hover:text-primary"
											on:click={() => openShare(share.resource_type, share.resource_id)}
										>
											{share.resource_name}
										</button>
										<div class="text-xs text-base-content/60">{share.resource_path}</div>
									</div>
								</div>
							</td>
							<td>
								<div>
									<div class="font-medium">{share.shared_by_name}</div>
									{#if share.shared_by_email}
										<div class="text-xs text-base-content/60">{share.shared_by_email}</div>
									{/if}
								</div>
							</td>
							<td>
								<span class="badge badge-ghost">{permissionLabel(share.permission)}</span>
							</td>
							<td>
								<div class="flex items-center justify-between gap-4">
									<span>{formatDate(share.created_at)}</span>
									<button
										type="button"
										class="btn btn-sm btn-outline"
										on:click={() => openShare(share.resource_type, share.resource_id)}
									>
										Open
									</button>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
