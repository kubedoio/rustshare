<script lang="ts">
	import { createQuery, createMutation } from '$lib/query-compat';
	import { page } from '$app/stores';
	import { queryClient } from '$lib/query-client';
	import { getAdminGroup, updateAdminGroup } from '$lib/api/admin';
	import GroupMemberList from '$lib/components/admin/GroupMemberList.svelte';

	let groupId = $derived($page.params.id);

	let groupQuery = $derived(
		createQuery({
			queryKey: ['admin', 'group', groupId],
			queryFn: () => getAdminGroup(groupId ?? ''),
			enabled: !!groupId
		})
	);

	let editName = $state('');
	let editDescription = $state('');
	let editing = $state(false);

	$effect(() => {
		if ($groupQuery.data && !editing) {
			editName = $groupQuery.data.name;
			editDescription = $groupQuery.data.description ?? '';
		}
	});

	const updateMutation = createMutation({
		mutationFn: () =>
			updateAdminGroup(groupId ?? '', {
				name: editName.trim() || undefined,
				description: editDescription.trim() || undefined
			}),
		onSuccess: () => {
			editing = false;
			queryClient.invalidateQueries({ queryKey: ['admin', 'group', groupId] });
			queryClient.invalidateQueries({ queryKey: ['admin', 'groups'] });
		}
	});

	function handleRefresh() {
		queryClient.invalidateQueries({ queryKey: ['admin', 'group', groupId] });
	}
</script>

<svelte:head>
	<title>Group Detail — Admin | RustShare</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex items-center gap-2">
		<a href="/admin/groups" class="btn btn-ghost btn-sm">
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
			Groups
		</a>
		<span class="text-base-content/40">/</span>
		<span class="font-medium">{$groupQuery.data?.name ?? groupId}</span>
	</div>

	{#if $groupQuery.isLoading}
		<div class="flex justify-center py-16">
			<span class="loading loading-lg loading-spinner"></span>
		</div>
	{:else if $groupQuery.isError}
		<div class="alert alert-error">
			Failed to load group: {$groupQuery.error instanceof Error
				? $groupQuery.error.message
				: 'Unknown error'}
		</div>
	{:else if $groupQuery.data}
		<!-- Group details card -->
		<div class="card bg-base-100 shadow">
			<div class="card-body">
				<div class="flex items-start justify-between">
					<h3 class="card-title">Group Details</h3>
					<button class="btn btn-ghost btn-sm" onclick={() => (editing = !editing)}>
						{editing ? 'Cancel' : 'Edit'}
					</button>
				</div>

				{#if editing}
					<form
						onsubmit={(e) => {
							e.preventDefault();
							$updateMutation.mutate();
						}}
						class="mt-2 space-y-4"
					>
						<div class="form-control">
							<label class="label" for="grp-name"><span class="label-text">Name</span></label>
							<input id="grp-name" type="text" class="input-bordered input" bind:value={editName} />
						</div>
						<div class="form-control">
							<label class="label" for="grp-desc"><span class="label-text">Description</span></label
							>
							<textarea
								id="grp-desc"
								class="textarea-bordered textarea"
								rows="3"
								bind:value={editDescription}></textarea>
						</div>
						{#if $updateMutation.isError}
							<div class="alert text-sm alert-error">
								{$updateMutation.error instanceof Error
									? $updateMutation.error.message
									: 'Failed to update'}
							</div>
						{/if}
						<button type="submit" class="btn btn-primary" disabled={$updateMutation.isPending}>
							{$updateMutation.isPending ? 'Saving...' : 'Save'}
						</button>
					</form>
				{:else}
					<dl class="mt-2 grid grid-cols-2 gap-4 text-sm">
						<div>
							<dt class="text-base-content/60">Name</dt>
							<dd class="mt-1 font-medium">{$groupQuery.data.name}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Members</dt>
							<dd class="mt-1 font-medium">{$groupQuery.data.member_count}</dd>
						</div>
						<div class="col-span-2">
							<dt class="text-base-content/60">Description</dt>
							<dd class="mt-1">{$groupQuery.data.description ?? '—'}</dd>
						</div>
						<div>
							<dt class="text-base-content/60">Created</dt>
							<dd class="mt-1">{new Date($groupQuery.data.created_at).toLocaleDateString()}</dd>
						</div>
					</dl>
				{/if}
			</div>
		</div>

		<!-- Members -->
		<div>
			<h3 class="mb-3 text-lg font-semibold">Members</h3>
			<GroupMemberList
				groupId={$groupQuery.data.id}
				members={$groupQuery.data.members}
				onRefresh={handleRefresh}
			/>
		</div>
	{/if}
</div>
