<script lang="ts">
	import { onMount } from 'svelte';
	import {
		createOrganizationGroup,
		deleteOrganizationGroup,
		listOrganizationGroups,
		type OrgGroup
	} from '$lib/api/organization-admin';

	const perPage = 50;
	let groups = $state<OrgGroup[]>([]);
	let total = $state(0);
	let page = $state(1);
	let name = $state('');
	let description = $state('');
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');

	async function load() {
		loading = true;
		error = '';
		try {
			const result = await listOrganizationGroups({ page, per_page: perPage });
			groups = result.groups ?? [];
			total = result.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to load groups';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void load();
	});

	async function createGroup(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		error = '';
		try {
			await createOrganizationGroup(name.trim(), description.trim());
			name = '';
			description = '';
			page = 1;
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to create group';
		} finally {
			busy = false;
		}
	}

	async function removeGroup(group: OrgGroup) {
		if (!confirm(`Delete group “${group.name}”? Group membership will be removed.`)) return;
		busy = true;
		error = '';
		try {
			await deleteOrganizationGroup(group.id);
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unable to delete group';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>Groups — Organization administration</title></svelte:head>
<section class="space-y-5">
	<h2 class="text-2xl font-semibold">Groups</h2>
	<form
		class="grid gap-3 rounded-lg border border-base-300 p-4 md:grid-cols-[1fr_2fr_auto] md:items-end"
		onsubmit={createGroup}
	>
		<div>
			<label class="label" for="group-name">Group name</label><input
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
		<button class="btn btn-primary" disabled={busy || !name.trim()}
			>{busy ? 'Saving…' : 'Create group'}</button
		>
	</form>
	{#if error}<div class="alert alert-error" role="alert">{error}</div>{/if}
	{#if loading}<p role="status" aria-live="polite">Loading groups…</p>
	{:else}
		<div class="overflow-x-auto rounded-lg border border-base-300">
			<table class="table table-zebra">
				<caption class="sr-only">Organization groups</caption>
				<thead><tr><th>Name</th><th>Description</th><th>Members</th><th>Actions</th></tr></thead>
				<tbody
					>{#each groups as group (group.id)}
						<tr
							><td
								><a class="link font-medium link-hover" href="/organization-admin/groups/{group.id}"
									>{group.name}</a
								></td
							><td>{group.description || '—'}</td><td>{group.member_count}</td><td
								class="flex gap-2"
								><a class="btn btn-ghost btn-xs" href="/organization-admin/groups/{group.id}"
									>Manage members</a
								><button
									class="btn btn-error btn-xs"
									disabled={busy}
									onclick={() => removeGroup(group)}>Delete</button
								></td
							></tr
						>
					{:else}<tr><td colspan="4" class="text-center">No groups yet.</td></tr>{/each}</tbody
				>
			</table>
		</div>
		<div class="flex items-center justify-between">
			<span>{total} groups · Page {page} of {Math.max(1, Math.ceil(total / perPage))}</span>
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
	{/if}
</section>
