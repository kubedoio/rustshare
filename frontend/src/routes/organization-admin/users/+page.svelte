<script lang="ts">
	import { tick } from 'svelte';
	import {
		bulkInviteOrganizationUsers,
		listOrganizationUsers,
		offboardOrganizationUsers,
		type BulkInviteRow,
		type OffboardRow,
		type OrgUser
	} from '$lib/api/organization-admin';

	const perPage = 50;
	let users = $state<OrgUser[]>([]);
	let total = $state(0);
	let page = $state(1);
	let searchInput = $state('');
	let search = $state('');
	let status = $state<'all' | 'active' | 'disabled'>('all');
	let loading = $state(false);
	let error = $state('');
	let selected = $state<OrgUser[]>([]);
	let inviteInput = $state('');
	let inviteBusy = $state(false);
	let inviteRows = $state<BulkInviteRow[]>([]);
	let invitePreviewInput = $state<string | null>(null);
	let inviteError = $state('');
	let offboardRows = $state<OffboardRow[] | null>(null);
	let offboardTargets = $state<OrgUser[]>([]);
	let offboardApplied = $state(false);
	let offboardBusy = $state(false);
	let offboardError = $state('');
	let dialog: HTMLDialogElement;
	let loadVersion = 0;

	async function loadUsers() {
		const requestVersion = ++loadVersion;
		const requestedPage = page;
		const requestedSearch = search;
		const requestedStatus = status;
		loading = true;
		error = '';
		try {
			const result = await listOrganizationUsers({
				page: requestedPage,
				per_page: perPage,
				search: requestedSearch,
				status: requestedStatus
			});
			if (requestVersion !== loadVersion) return;
			users = result.users ?? [];
			total = result.total;
		} catch (e) {
			if (requestVersion !== loadVersion) return;
			error = e instanceof Error ? e.message : 'Unable to load users';
		} finally {
			if (requestVersion === loadVersion) loading = false;
		}
	}

	$effect(() => {
		void page;
		void search;
		void status;
		loadUsers();
	});

	function runSearch(event: SubmitEvent) {
		event.preventDefault();
		page = 1;
		search = searchInput.trim();
	}

	function changeStatus(event: Event) {
		status = (event.currentTarget as HTMLSelectElement).value as typeof status;
		page = 1;
	}

	function toggleUser(user: OrgUser) {
		selected = selected.some((item) => item.id === user.id)
			? selected.filter((item) => item.id !== user.id)
			: [...selected, user];
	}

	async function runBulkInvite(dryRun: boolean) {
		const parsed = inviteInput
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean)
			.map((email) => ({ email, display_name: email.split('@')[0], group_ids: [] as string[] }));
		inviteBusy = true;
		inviteError = '';
		try {
			const result = await bulkInviteOrganizationUsers(parsed, dryRun);
			inviteRows = result.rows;
			if (dryRun) invitePreviewInput = inviteInput;
			else {
				invitePreviewInput = null;
				await loadUsers();
			}
		} catch (e) {
			inviteError = e instanceof Error ? e.message : 'Invitation request failed';
		} finally {
			inviteBusy = false;
		}
	}

	async function previewOffboarding() {
		offboardBusy = true;
		offboardError = '';
		try {
			offboardTargets = [...selected];
			offboardApplied = false;
			offboardRows = (
				await offboardOrganizationUsers(
					selected.map((user) => user.id),
					true
				)
			).rows;
			await tick();
			dialog.showModal();
		} catch (e) {
			offboardError = e instanceof Error ? e.message : 'Offboarding preview failed';
		} finally {
			offboardBusy = false;
		}
	}

	async function applyOffboarding() {
		offboardBusy = true;
		offboardError = '';
		try {
			offboardRows = (
				await offboardOrganizationUsers(
					offboardTargets.map((user) => user.id),
					false
				)
			).rows;
			offboardApplied = true;
			selected = [];
			page = 1;
			await loadUsers();
		} catch (e) {
			offboardError = e instanceof Error ? e.message : 'Offboarding failed';
		} finally {
			offboardBusy = false;
		}
	}
</script>

<svelte:head><title>Users — Organization administration</title></svelte:head>

<section class="space-y-8">
	<div>
		<h2 class="mb-3 text-2xl font-semibold">Users</h2>
		<form class="mb-4 flex flex-wrap gap-2" onsubmit={runSearch} role="search">
			<label class="sr-only" for="org-user-search">Search users</label>
			<input
				id="org-user-search"
				class="input-bordered input input-sm"
				bind:value={searchInput}
				placeholder="Search name or email"
			/>
			<label class="sr-only" for="org-user-status">User status</label>
			<select
				id="org-user-status"
				class="select-bordered select select-sm"
				value={status}
				onchange={changeStatus}
			>
				<option value="all">All statuses</option><option value="active">Enabled</option><option
					value="disabled">Disabled</option
				>
			</select>
			<button class="btn btn-primary btn-sm" type="submit">Search</button>
		</form>
		{#if loading}<p role="status" aria-live="polite">Loading users…</p>
		{:else if error}<div class="alert alert-error" role="alert">{error}</div>
		{:else}
			<div class="overflow-x-auto rounded-lg border border-base-300">
				<table class="table table-zebra">
					<caption class="sr-only">Organization users</caption>
					<thead><tr><th>Select</th><th>Name</th><th>Email</th><th>Status</th></tr></thead>
					<tbody>
						{#each users as user (user.id)}
							<tr>
								<td
									><input
										type="checkbox"
										class="checkbox checkbox-sm"
										aria-label="Select {user.display_name || user.username}"
										checked={selected.some((item) => item.id === user.id)}
										onchange={() => toggleUser(user)}
									/></td
								>
								<td>{user.display_name || user.username}</td><td>{user.email}</td>
								<td>{user.disabled_at ? 'Disabled' : 'Enabled'}</td>
							</tr>
						{:else}<tr><td colspan="4" class="text-center">No users found.</td></tr>{/each}
					</tbody>
				</table>
			</div>
			<div class="mt-3 flex items-center justify-between gap-3">
				<p>{total} users · Page {page} of {Math.max(1, Math.ceil(total / perPage))}</p>
				<div class="flex gap-2">
					<button class="btn btn-sm" disabled={page <= 1} onclick={() => (page -= 1)}
						>Previous</button
					><button class="btn btn-sm" disabled={page * perPage >= total} onclick={() => (page += 1)}
						>Next</button
					>
				</div>
			</div>
		{/if}
		{#if selected.length}
			<div class="mt-4 flex flex-wrap items-center gap-3">
				<span>{selected.length} selected</span>
				<button class="btn btn-error btn-sm" disabled={offboardBusy} onclick={previewOffboarding}
					>{offboardBusy ? 'Preparing…' : 'Preview offboarding'}</button
				>
			</div>
		{/if}
		{#if offboardError}<p class="mt-2 text-error" role="alert">{offboardError}</p>{/if}
	</div>

	<section class="rounded-lg border border-base-300 p-4" aria-labelledby="bulk-title">
		<h3 id="bulk-title" class="text-xl font-semibold">Bulk invitations</h3>
		<p class="my-2 text-sm text-base-content/70">
			Enter one email address per line. Dry-run first; each row reports its outcome.
		</p>
		<label class="sr-only" for="bulk-invite-emails">Email addresses</label>
		<textarea
			id="bulk-invite-emails"
			class="textarea-bordered textarea min-h-28 w-full"
			bind:value={inviteInput}
			placeholder="alex@example.org&#10;sam@example.org"></textarea>
		<div class="mt-2 flex gap-2">
			<button
				class="btn btn-sm"
				disabled={inviteBusy || !inviteInput.trim()}
				onclick={() => runBulkInvite(true)}>Dry run</button
			>
			<button
				class="btn btn-primary btn-sm"
				disabled={inviteBusy || !inviteInput.trim() || invitePreviewInput !== inviteInput}
				onclick={() => runBulkInvite(false)}>Send invitations</button
			>
		</div>
		{#if invitePreviewInput !== null && invitePreviewInput !== inviteInput}<p
				class="mt-2 text-warning"
				role="status"
			>
				Input changed; run a new dry run before sending.
			</p>{/if}
		{#if inviteError}<p class="mt-2 text-error" role="alert">{inviteError}</p>{/if}
		{#if inviteRows.length}<ul class="mt-3 space-y-1" aria-live="polite">
				{#each inviteRows as row (row.row)}<li>Row {row.row}: {row.outcome}</li>{/each}
			</ul>{/if}
	</section>
</section>

<dialog
	bind:this={dialog}
	class="modal"
	aria-labelledby="offboard-title"
	aria-describedby="offboard-description"
>
	<div class="modal-box">
		<h2 id="offboard-title" class="text-xl font-bold">Confirm user offboarding</h2>
		<p id="offboard-description" class="my-3">
			This disables selected accounts and revokes access. Authored content and its ownership are
			preserved.
		</p>
		{#if offboardRows}<ul class="max-h-48 overflow-y-auto" aria-live="polite">
				{#each offboardRows as row (row.user_id)}<li>
						{offboardTargets.find((user) => user.id === row.user_id)?.display_name ||
							offboardTargets.find((user) => user.id === row.user_id)?.email ||
							row.user_id}:
						{row.outcome}{#if offboardApplied}; {row.shares_revoked} shares revoked;
							{row.group_memberships_removed} group memberships removed{:else}; share and group
							impact counts are not calculated during dry run{/if}
					</li>{/each}
			</ul>{/if}
		{#if offboardError}<p class="text-error" role="alert">{offboardError}</p>{/if}
		<div class="modal-action">
			<button class="btn" disabled={offboardBusy} onclick={() => dialog.close()}>Close</button>
			<button
				class="btn btn-error"
				disabled={offboardBusy || offboardApplied || !offboardRows?.length}
				onclick={applyOffboarding}
				>{offboardBusy ? 'Applying…' : offboardApplied ? 'Applied' : 'Confirm disable'}</button
			>
		</div>
	</div>
</dialog>
