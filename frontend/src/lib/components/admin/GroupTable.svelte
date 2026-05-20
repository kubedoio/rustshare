<script lang="ts">
	import type { AdminGroup } from '$lib/api/admin';

	let {
		groups = [],
		onDelete = () => {},
		onCreate = () => {}
	}: {
		groups?: AdminGroup[];
		onDelete?: (group: AdminGroup) => void;
		onCreate?: () => void;
	} = $props();

	let confirmDelete = $state<AdminGroup | null>(null);

	function handleDelete(group: AdminGroup) {
		confirmDelete = group;
	}

	function confirmAndDelete() {
		if (confirmDelete) {
			onDelete(confirmDelete);
			confirmDelete = null;
		}
	}
</script>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<span class="text-sm text-base-content/60"
			>{groups.length} group{groups.length !== 1 ? 's' : ''}</span
		>
		<button class="btn btn-sm btn-primary" onclick={onCreate}>New Group</button>
	</div>

	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table w-full table-zebra">
			<thead>
				<tr>
					<th class="font-data">Name</th>
					<th class="font-data">Description</th>
					<th class="font-data">Members</th>
					<th class="font-data">Created</th>
					<th class="font-data">Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each groups as group (group.id)}
					<tr>
						<td class="font-data font-medium">
							<a href="/admin/groups/{group.id}" class="link link-hover">{group.name}</a>
						</td>
						<td class="font-data text-sm text-base-content/70">{group.description ?? '—'}</td>
						<td>
							<span class="badge badge-ghost font-data badge-sm tabular-nums"
								>{group.member_count}</span
							>
						</td>
						<td class="font-data text-sm text-base-content/60">
							{new Date(group.created_at).toLocaleDateString()}
						</td>
						<td>
							<div class="flex gap-1">
								<a href="/admin/groups/{group.id}" class="btn btn-ghost btn-xs">Edit</a>
								<button
									class="btn text-error btn-ghost btn-xs"
									onclick={() => handleDelete(group)}
								>
									Delete
								</button>
							</div>
						</td>
					</tr>
				{/each}
				{#if groups.length === 0}
					<tr>
						<td colspan="5" class="py-8 text-center text-base-content/50">No groups found</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

{#if confirmDelete}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Delete Group</h3>
			<p class="py-4">
				Are you sure you want to delete group <strong>{confirmDelete.name}</strong>? This action
				cannot be undone.
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" onclick={() => (confirmDelete = null)}>Cancel</button>
				<button class="btn btn-error" onclick={confirmAndDelete}>Delete</button>
			</div>
		</div>
		<div class="modal-backdrop" onclick={() => (confirmDelete = null)} role="presentation"></div>
	</div>
{/if}
