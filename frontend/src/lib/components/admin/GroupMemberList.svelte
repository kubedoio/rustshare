<script lang="ts">
	import { createMutation } from '@tanstack/svelte-query';
	import { addGroupMember, removeGroupMember, type GroupMember, type AdminUser } from '$lib/api/admin';
	import UserSearchInput from '$lib/components/common/UserSearchInput.svelte';

	export let groupId: string;
	export let members: GroupMember[] = [];
	export let onRefresh: () => void = () => {};

	let confirmRemove: GroupMember | null = null;

	$: memberIds = members.map((m) => m.user_id);

	const addMutation = createMutation({
		mutationFn: (userId: string) => addGroupMember(groupId, userId),
		onSuccess: () => onRefresh()
	});

	const removeMutation = createMutation({
		mutationFn: (userId: string) => removeGroupMember(groupId, userId),
		onSuccess: () => {
			confirmRemove = null;
			onRefresh();
		}
	});

	function handleSelectUser(user: AdminUser) {
		$addMutation.mutate(user.id);
	}
</script>

<div class="space-y-4">
	<div class="card bg-base-100 shadow">
		<div class="card-body">
			<h3 class="card-title text-base">Add Member</h3>
			<div class="max-w-sm">
				<UserSearchInput
					placeholder="Search users to add..."
					excludeIds={memberIds}
					onselect={handleSelectUser}
				/>
			</div>
			{#if $addMutation.isError}
				<div class="alert alert-error text-sm mt-2">
					{$addMutation.error instanceof Error ? $addMutation.error.message : 'Failed to add member'}
				</div>
			{/if}
		</div>
	</div>

	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table table-zebra w-full">
			<thead>
				<tr>
					<th>Username</th>
					<th>Email</th>
					<th>Added</th>
					<th>Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each members as member (member.id)}
					<tr>
						<td class="font-medium">{member.username}</td>
						<td class="text-sm text-base-content/70">{member.email}</td>
						<td class="text-sm text-base-content/60">
							{new Date(member.added_at).toLocaleDateString()}
						</td>
						<td>
							<button
								class="btn btn-ghost btn-xs text-error"
								on:click={() => (confirmRemove = member)}
							>
								Remove
							</button>
						</td>
					</tr>
				{/each}
				{#if members.length === 0}
					<tr>
						<td colspan="4" class="text-center text-base-content/50 py-8">No members yet</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

{#if confirmRemove}
	<div class="modal modal-open">
		<div class="modal-box">
			<h3 class="font-bold text-lg">Remove Member</h3>
			<p class="py-4">
				Remove <strong>{confirmRemove.username}</strong> from this group?
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmRemove = null)}>Cancel</button>
				<button
					class="btn btn-error"
					on:click={() => confirmRemove && $removeMutation.mutate(confirmRemove.user_id)}
					disabled={$removeMutation.isPending}
				>
					{$removeMutation.isPending ? 'Removing...' : 'Remove'}
				</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmRemove = null)} role="presentation"></div>
	</div>
{/if}
