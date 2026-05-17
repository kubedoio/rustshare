<script lang="ts">
	import { createMutation } from '$lib/query-compat';
	import type { AdminUser } from '$lib/api/admin';
	import { disableAdminUser, enableAdminUser, deleteAdminUser } from '$lib/api/admin';

	let {
		users = [],
		total = 0,
		page = 1,
		perPage = 20,
		onPageChange = () => {},
		onSearch = () => {},
		onStatusFilter = () => {},
		onRefresh = () => {}
	}: {
		users?: AdminUser[];
		total?: number;
		page?: number;
		perPage?: number;
		onPageChange?: (page: number) => void;
		onSearch?: (query: string) => void;
		onStatusFilter?: (status: string) => void;
		onRefresh?: () => void;
	} = $props();

	let searchValue = $state('');
	let statusFilter = '';
	let searchTimeout: ReturnType<typeof setTimeout>;
	let confirmDelete = $state<string | null>(null);

	const totalPages = Math.ceil(total / perPage);

	function handleSearchInput(e: Event) {
		const val = (e.target as HTMLInputElement).value;
		searchValue = val;
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(() => onSearch(val), 300);
	}

	function handleStatusChange(e: Event) {
		statusFilter = (e.target as HTMLSelectElement).value;
		onStatusFilter(statusFilter);
	}

	const disableMutation = createMutation({
		mutationFn: (id: string) => disableAdminUser(id),
		onSuccess: () => onRefresh()
	});

	const enableMutation = createMutation({
		mutationFn: (id: string) => enableAdminUser(id),
		onSuccess: () => onRefresh()
	});

	const deleteMutation = createMutation({
		mutationFn: (id: string) => deleteAdminUser(id),
		onSuccess: () => {
			confirmDelete = null;
			onRefresh();
		}
	});

	function formatDate(dateStr: string) {
		return new Date(dateStr).toLocaleDateString();
	}

	function formatBytes(bytes: number) {
		if (bytes === 0) return 'Unlimited';
		const gb = bytes / (1024 * 1024 * 1024);
		return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
	}
</script>

<div class="space-y-4">
	<!-- Filters -->
	<div class="flex flex-wrap items-center gap-3">
		<input
			type="text"
			placeholder="Search users..."
			class="input-bordered input input-sm w-64"
			value={searchValue}
			on:input={handleSearchInput}
		/>
		<select class="select-bordered select select-sm" on:change={handleStatusChange}>
			<option value="">All statuses</option>
			<option value="active">Active</option>
			<option value="disabled">Disabled</option>
		</select>
		<span class="ml-auto text-sm text-base-content/60">{total} user{total !== 1 ? 's' : ''}</span>
	</div>

	<!-- Table -->
	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table w-full table-zebra">
			<thead>
				<tr>
					<th class="font-data">Username</th>
					<th class="font-data">Email</th>
					<th class="font-data">Status</th>
					<th class="font-data">Role</th>
					<th class="font-data">Quota</th>
					<th class="font-data">Created</th>
					<th class="font-data">Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each users as user (user.id)}
					<tr>
						<td class="font-data font-medium">{user.username}</td>
						<td class="font-data text-sm text-base-content/70">{user.email}</td>
						<td>
							{#if user.disabled_at}
								<span class="badge badge-sm badge-error">Disabled</span>
							{:else}
								<span class="badge badge-sm badge-success">Active</span>
							{/if}
						</td>
						<td>
							{#if user.is_admin}
								<span class="badge badge-sm badge-warning">Admin</span>
							{:else}
								<span class="badge badge-ghost badge-sm">User</span>
							{/if}
						</td>
						<td class="font-data text-sm tabular-nums">{formatBytes(user.storage_quota_bytes)}</td>
						<td class="font-data text-sm text-base-content/60">{formatDate(user.created_at)}</td>
						<td>
							<div class="flex gap-1">
								<a href="/admin/users/{user.id}" class="btn btn-ghost btn-xs">Edit</a>
								{#if user.disabled_at}
									<button
										class="btn text-success btn-ghost btn-xs"
										on:click={() => $enableMutation.mutate(user.id)}
										disabled={$enableMutation.isPending}
									>
										Enable
									</button>
								{:else}
									<button
										class="btn text-warning btn-ghost btn-xs"
										on:click={() => $disableMutation.mutate(user.id)}
										disabled={$disableMutation.isPending}
									>
										Disable
									</button>
								{/if}
								<button
									class="btn text-error btn-ghost btn-xs"
									on:click={() => (confirmDelete = user.id)}
								>
									Delete
								</button>
							</div>
						</td>
					</tr>
				{/each}
				{#if users.length === 0}
					<tr>
						<td colspan="7" class="py-8 text-center text-base-content/50">No users found</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	<!-- Pagination -->
	{#if totalPages > 1}
		<div class="flex justify-center gap-2">
			<button
				class="btn btn-ghost btn-sm"
				disabled={page <= 1}
				on:click={() => onPageChange(page - 1)}
			>
				Previous
			</button>
			<span class="flex items-center px-2 text-sm">Page {page} of {totalPages}</span>
			<button
				class="btn btn-ghost btn-sm"
				disabled={page >= totalPages}
				on:click={() => onPageChange(page + 1)}
			>
				Next
			</button>
		</div>
	{/if}
</div>

<!-- Delete confirmation modal -->
{#if confirmDelete}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Delete User</h3>
			<p class="py-4">Are you sure you want to delete this user? This action cannot be undone.</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmDelete = null)}>Cancel</button>
				<button
					class="btn btn-error"
					on:click={() => confirmDelete && $deleteMutation.mutate(confirmDelete)}
					disabled={$deleteMutation.isPending}
				>
					{$deleteMutation.isPending ? 'Deleting...' : 'Delete'}
				</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmDelete = null)} role="presentation"></div>
	</div>
{/if}
