<script lang="ts">
	import { page as route } from '$app/stores';
	import {
		getOrganizationGroup,
		listOrganizationUsers,
		removeOrganizationGroupMember,
		updateOrganizationGroup,
		addOrganizationGroupMember,
		type OrgMember,
		type OrgUser
	} from '$lib/api/organization-admin';

	const perPage = 50;
	let groupId = $derived($route.params.id ?? '');
	let group = $state<{ id: string; name: string; description: string | null } | null>(null);
	let members = $state<OrgMember[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');
	let name = $state('');
	let description = $state('');
	let memberSearch = $state('');
	let matches = $state<OrgUser[]>([]);
	let selectedUser = $state('');

	async function load() {
		loading = true;
		error = '';
		try {
			const result = await getOrganizationGroup(groupId, { page, per_page: perPage });
			group = result;
			name = result.name;
			description = result.description ?? '';
			members = result.members;
			total = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to load group';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (groupId) void load();
	});

	async function saveGroup(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = '';
		try {
			await updateOrganizationGroup(groupId, name.trim(), description.trim());
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to update group';
		} finally {
			busy = false;
		}
	}

	async function searchMembers(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = '';
		try {
			const result = await listOrganizationUsers({
				page: 1,
				per_page: 20,
				search: memberSearch.trim(),
				status: 'active'
			});
			matches = result.users ?? [];
			selectedUser = '';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to search users';
		} finally {
			busy = false;
		}
	}

	async function addMember() {
		if (!selectedUser) return;
		busy = true;
		error = '';
		try {
			await addOrganizationGroupMember(groupId, selectedUser);
			matches = [];
			selectedUser = '';
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to add member';
		} finally {
			busy = false;
		}
	}

	async function removeMember(userId: string) {
		busy = true;
		error = '';
		try {
			await removeOrganizationGroupMember(groupId, userId);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to remove member';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>{group?.name ?? 'Group'} — Organization administration</title></svelte:head>
<section class="space-y-6">
	<a class="link" href="/organization-admin/groups">← All groups</a>
	{#if loading}<p role="status" aria-live="polite">Loading group…</p>
	{:else if error && !group}<div class="alert alert-error" role="alert">{error}</div>
	{:else if group}
		<h2 class="text-2xl font-semibold">Manage group: {group.name}</h2>
		{#if error}<div class="alert alert-error" role="alert">{error}</div>{/if}
		<form
			class="grid gap-3 rounded-lg border border-base-300 p-4 md:grid-cols-[1fr_2fr_auto] md:items-end"
			onsubmit={saveGroup}
		>
			<div>
				<label class="label" for="group-name">Name</label><input
					id="group-name"
					class="input-bordered input w-full"
					required
					maxlength="120"
					bind:value={name}
				/>
			</div>
			<div>
				<label class="label" for="group-description">Description</label><input
					id="group-description"
					class="input-bordered input w-full"
					bind:value={description}
				/>
			</div>
			<button class="btn btn-primary" disabled={busy || !name.trim()}>Save group</button>
		</form>
		<section aria-labelledby="members-title" class="space-y-3">
			<h3 id="members-title" class="text-xl font-semibold">Members ({total})</h3>
			<form class="flex flex-wrap gap-2" onsubmit={searchMembers} role="search">
				<label class="sr-only" for="member-search">Search organization users</label>
				<input
					id="member-search"
					class="input-bordered input input-sm"
					bind:value={memberSearch}
					placeholder="Find enabled user by name or email"
				/>
				<button class="btn btn-sm" disabled={busy}>Search users</button>
			</form>
			{#if matches.length}
				<div class="flex flex-wrap items-center gap-2">
					<label class="sr-only" for="member-candidate">User to add</label>
					<select
						id="member-candidate"
						class="select-bordered select select-sm"
						bind:value={selectedUser}
					>
						<option value="">Choose a user</option
						>{#each matches as candidate (candidate.id)}<option value={candidate.id}
								>{candidate.display_name} · {candidate.email}</option
							>{/each}
					</select>
					<button
						class="btn btn-primary btn-sm"
						disabled={busy || !selectedUser}
						onclick={addMember}>Add to group</button
					>
				</div>
			{:else if memberSearch}<p class="text-sm text-base-content/70">
					No matching enabled users. Run search to refresh results.
				</p>{/if}
			<div class="overflow-x-auto rounded-lg border border-base-300">
				<table class="table table-zebra">
					<caption class="sr-only">Members of {group.name}</caption><thead
						><tr><th>Name</th><th>Email</th><th>Added</th><th>Action</th></tr></thead
					><tbody>
						{#each members as member (member.user_id)}<tr
								><td>{member.username}</td><td>{member.email}</td><td
									>{new Date(member.added_at).toLocaleDateString()}</td
								><td
									><button
										class="btn btn-error btn-xs"
										disabled={busy}
										onclick={() => removeMember(member.user_id)}>Remove</button
									></td
								></tr
							>
						{:else}<tr><td colspan="4" class="text-center">No members.</td></tr>{/each}
					</tbody>
				</table>
			</div>
			<div class="flex items-center justify-between">
				<span>Page {page} of {Math.max(1, Math.ceil(total / perPage))}</span>
				<div class="flex gap-2">
					<button
						class="btn btn-sm"
						disabled={page <= 1 || busy}
						onclick={() => {
							page -= 1;
							void load();
						}}>Previous</button
					><button
						class="btn btn-sm"
						disabled={page * perPage >= total || busy}
						onclick={() => {
							page += 1;
							void load();
						}}>Next</button
					>
				</div>
			</div>
		</section>
	{/if}
</section>
