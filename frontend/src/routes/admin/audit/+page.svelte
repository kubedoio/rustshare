<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { listAuditLog } from '$lib/api/admin';
	import AuditTable from '$lib/components/admin/AuditTable.svelte';

	let currentPage = $state(1);
	let filters: { type?: string; user_id?: string; from?: string; to?: string } = $state({});

	const PER_PAGE = 50;

	let auditQuery = $derived(createQuery({
		queryKey: ['admin', 'audit', currentPage, filters],
		queryFn: () =>
			listAuditLog({
				page: currentPage,
				per_page: PER_PAGE,
				...filters
			})
	}));

	function handleFilterChange(f: typeof filters) {
		filters = f;
		currentPage = 1;
	}

	function handlePageChange(p: number) {
		currentPage = p;
	}
</script>

<svelte:head>
	<title>Audit Log — Admin | RustShare</title>
</svelte:head>

<div class="space-y-4">
	<h2 class="text-2xl font-bold">Audit Log</h2>

	{#if $auditQuery.isLoading}
		<div class="flex justify-center py-16">
			<span class="loading loading-lg loading-spinner"></span>
		</div>
	{:else if $auditQuery.isError}
		<div class="alert alert-error">
			Failed to load audit log: {$auditQuery.error instanceof Error
				? $auditQuery.error.message
				: 'Unknown error'}
		</div>
	{:else if $auditQuery.data}
		<AuditTable
			entries={$auditQuery.data.entries}
			total={$auditQuery.data.total}
			page={currentPage}
			perPage={PER_PAGE}
			onPageChange={handlePageChange}
			onFilterChange={handleFilterChange}
		/>
	{/if}
</div>
