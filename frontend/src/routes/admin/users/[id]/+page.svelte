<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { page } from '$app/stores';
	import { queryClient } from '$lib/query-client';
	import { getAdminUser } from '$lib/api/admin';
	import UserDetailForm from '$lib/components/admin/UserDetailForm.svelte';

	$: userId = $page.params.id;

	$: userQuery = createQuery({
		queryKey: ['admin', 'user', userId],
		queryFn: () => getAdminUser(userId ?? ''),
		enabled: !!userId
	});

	function handleRefresh() {
		queryClient.invalidateQueries({ queryKey: ['admin', 'user', userId] });
		queryClient.invalidateQueries({ queryKey: ['admin', 'users'] });
	}
</script>

<svelte:head>
	<title>Edit User — Admin | RustShare</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center gap-2">
		<a href="/admin/users" class="btn btn-ghost btn-sm">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				class="h-4 w-4"
				fill="none"
				viewBox="0 0 24 24"
				stroke="currentColor"
				stroke-width="1.5"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18"
				/>
			</svg>
			Users
		</a>
		<span class="text-base-content/40">/</span>
		<span class="font-medium">{$userQuery.data?.username ?? userId}</span>
	</div>

	{#if $userQuery.isLoading}
		<div class="flex justify-center py-16">
			<span class="loading loading-lg loading-spinner"></span>
		</div>
	{:else if $userQuery.isError}
		<div class="alert alert-error">
			Failed to load user: {$userQuery.error instanceof Error
				? $userQuery.error.message
				: 'Unknown error'}
		</div>
	{:else if $userQuery.data}
		<UserDetailForm user={$userQuery.data} onRefresh={handleRefresh} />
	{/if}
</div>
