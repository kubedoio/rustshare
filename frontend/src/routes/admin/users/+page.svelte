<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import { listAdminUsers } from '$lib/api/admin';
	import UserTable from '$lib/components/admin/UserTable.svelte';
	import CreateUserModal from '$lib/components/admin/CreateUserModal.svelte';

	let currentPage = 1;
	let searchQuery = '';
	let statusFilter = '';
	let showCreateModal = false;

	const PER_PAGE = 20;

	$: usersQuery = createQuery({
		queryKey: ['admin', 'users', currentPage, searchQuery, statusFilter],
		queryFn: () =>
			listAdminUsers({
				page: currentPage,
				per_page: PER_PAGE,
				search: searchQuery || undefined,
				status: statusFilter || undefined
			})
	});

	function handleSearch(q: string) {
		searchQuery = q;
		currentPage = 1;
	}

	function handleStatusFilter(s: string) {
		statusFilter = s;
		currentPage = 1;
	}

	function handlePageChange(p: number) {
		currentPage = p;
	}

	function handleRefresh() {
		queryClient.invalidateQueries({ queryKey: ['admin', 'users'] });
	}

	function handleCreated() {
		showCreateModal = false;
		handleRefresh();
	}
</script>

<svelte:head>
	<title>Users — Admin | RustShare</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<h2 class="text-2xl font-bold">Users</h2>
		<button class="btn btn-primary" on:click={() => (showCreateModal = true)}> + New User </button>
	</div>

	{#if $usersQuery.isLoading}
		<div class="flex justify-center py-16">
			<span class="loading loading-lg loading-spinner"></span>
		</div>
	{:else if $usersQuery.isError}
		<div class="alert alert-error">
			Failed to load users: {$usersQuery.error instanceof Error
				? $usersQuery.error.message
				: 'Unknown error'}
		</div>
	{:else if $usersQuery.data}
		<UserTable
			users={$usersQuery.data.users}
			total={$usersQuery.data.total}
			page={currentPage}
			perPage={PER_PAGE}
			onPageChange={handlePageChange}
			onSearch={handleSearch}
			onStatusFilter={handleStatusFilter}
			onRefresh={handleRefresh}
		/>
	{/if}
</div>

<CreateUserModal
	open={showCreateModal}
	onClose={() => (showCreateModal = false)}
	onCreated={handleCreated}
/>
