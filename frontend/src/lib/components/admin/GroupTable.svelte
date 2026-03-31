<script lang="ts">
	import type { AdminGroup } from '$lib/api/admin';

	export let groups: AdminGroup[] = [];
	export let onDelete: (group: AdminGroup) => void = () => {};
	export let onCreate: () => void = () => {};

	let confirmDelete: AdminGroup | null = null;

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
	<div class="flex justify-between items-center">
		<span class="text-sm text-base-content/60">{groups.length} group{groups.length !== 1 ? 's' : ''}</span>
		<button class="btn btn-primary btn-sm" on:click={onCreate}>New Group</button>
	</div>

	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table table-zebra w-full">
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
						<td class="font-medium font-data">
							<a href="/admin/groups/{group.id}" class="link link-hover">{group.name}</a>
						</td>
						<td class="text-sm text-base-content/70 font-data">{group.description ?? '—'}</td>
						<td>
							<span class="badge badge-ghost badge-sm font-data tabular-nums">{group.member_count}</span>
						</td>
						<td class="text-sm text-base-content/60 font-data">
							{new Date(group.created_at).toLocaleDateString()}
						</td>
						<td>
							<div class="flex gap-1">
								<a href="/admin/groups/{group.id}" class="btn btn-ghost btn-xs">Edit</a>
								<button
									class="btn btn-ghost btn-xs text-error"
									on:click={() => handleDelete(group)}
								>
									Delete
								</button>
							</div>
						</td>
					</tr>
				{/each}
				{#if groups.length === 0}
					<tr>
						<td colspan="5" class="text-center text-base-content/50 py-8">No groups found</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

{#if confirmDelete}
	<div class="modal modal-open">
		<div class="modal-box">
			<h3 class="font-bold text-lg">Delete Group</h3>
			<p class="py-4">
				Are you sure you want to delete group <strong>{confirmDelete.name}</strong>? This action
				cannot be undone.
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmDelete = null)}>Cancel</button>
				<button class="btn btn-error" on:click={confirmAndDelete}>Delete</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmDelete = null)} role="presentation"></div>
	</div>
{/if}
