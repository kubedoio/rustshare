<script lang="ts">
	import { createQuery, createMutation } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import { listAdminGroups, deleteAdminGroup, type AdminGroup } from '$lib/api/admin';
	import GroupTable from '$lib/components/admin/GroupTable.svelte';
	import CreateGroupModal from '$lib/components/admin/CreateGroupModal.svelte';

	let showCreateModal = false;

	const groupsQuery = createQuery({
		queryKey: ['admin', 'groups'],
		queryFn: listAdminGroups
	});

	const deleteMutation = createMutation({
		mutationFn: (id: string) => deleteAdminGroup(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
		}
	});

	function handleDelete(group: AdminGroup) {
		$deleteMutation.mutate(group.id);
	}

	function handleCreated() {
		showCreateModal = false;
		queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
	}
</script>

<svelte:head>
	<title>Groups — Admin | RustShare</title>
</svelte:head>

<div class="space-y-4">
	<h2 class="text-2xl font-bold">Groups</h2>

	{#if $groupsQuery.isLoading}
		<div class="flex justify-center py-16"><span class="loading loading-spinner loading-lg"></span></div>
	{:else if $groupsQuery.isError}
		<div class="alert alert-error">
			Failed to load groups: {$groupsQuery.error instanceof Error ? $groupsQuery.error.message : 'Unknown error'}
		</div>
	{:else if $groupsQuery.data}
		<GroupTable
			groups={$groupsQuery.data}
			onDelete={handleDelete}
			onCreate={() => (showCreateModal = true)}
		/>
	{/if}
</div>

<CreateGroupModal
	open={showCreateModal}
	onClose={() => (showCreateModal = false)}
	onCreated={handleCreated}
/>
