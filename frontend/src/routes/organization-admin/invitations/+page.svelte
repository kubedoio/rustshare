<script lang="ts">
	import { onMount } from 'svelte';
	import {
		cancelOrganizationInvitation,
		listOrganizationInvitations,
		resendOrganizationInvitation,
		type OrganizationInvitation
	} from '$lib/api/organization-admin';

	const perPage = 50;
	let invitations = $state<OrganizationInvitation[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(true);
	let busyId = $state('');
	let error = $state('');
	let loadVersion = 0;
	let notice = $state('');
	const retryKeys = new Map<string, string>();

	async function load(requestedPage = page) {
		const requestVersion = ++loadVersion;
		loading = true;
		error = '';
		try {
			const result = await listOrganizationInvitations({ page: requestedPage, per_page: perPage });
			if (requestVersion !== loadVersion || requestedPage !== page) return;
			invitations = result.invitations ?? [];
			total = result.total;
		} catch (e) {
			if (requestVersion !== loadVersion || requestedPage !== page) return;
			error = e instanceof Error ? e.message : 'Unable to load invitations';
		} finally {
			if (requestVersion === loadVersion && requestedPage === page) loading = false;
		}
	}

	onMount(() => {
		void load();
	});

	async function resend(invitation: OrganizationInvitation) {
		busyId = invitation.id;
		error = '';
		notice = '';
		const key = retryKeys.get(invitation.id) ?? crypto.randomUUID();
		retryKeys.set(invitation.id, key);
		try {
			const result = await resendOrganizationInvitation(invitation.id, key);
			retryKeys.delete(invitation.id);
			notice = result.replayed
				? 'Previous resend result recovered.'
				: `Delivery ${result.delivery_status}.`;
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Resend failed. Retry is safe.';
		} finally {
			busyId = '';
		}
	}

	async function cancel(invitation: OrganizationInvitation) {
		if (!confirm(`Cancel the pending invitation for ${invitation.recipient_email}?`)) return;
		busyId = invitation.id;
		error = '';
		notice = '';
		try {
			const result = await cancelOrganizationInvitation(invitation.id);
			notice = result.changed
				? `Invitation cancelled; ${result.group_assignments_removed} pending group assignments removed.`
				: 'Invitation was already inactive.';
			await load();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Cancellation failed';
		} finally {
			busyId = '';
		}
	}
</script>

<svelte:head><title>Invitations — Organization administration</title></svelte:head>
<section class="space-y-4">
	<h2 class="text-2xl font-semibold">Invitations</h2>
	<p class="text-sm text-base-content/70">Invitation tokens and links are never shown here.</p>
	{#if error}<div class="alert alert-error" role="alert">{error}</div>{/if}
	{#if notice}<p class="alert alert-info" role="status" aria-live="polite">{notice}</p>{/if}
	{#if loading}<p role="status" aria-live="polite">Loading invitations…</p>
	{:else}
		<div class="overflow-x-auto rounded-lg border border-base-300">
			<table class="table table-zebra">
				<caption class="sr-only">Organization invitations</caption>
				<thead
					><tr
						><th>Recipient</th><th>Status</th><th>Sent / created</th><th>Expires</th><th>Actions</th
						></tr
					></thead
				>
				<tbody
					>{#each invitations as invitation (invitation.id)}
						<tr
							><td
								>{invitation.recipient_display_name || invitation.recipient_email}<br /><span
									class="text-xs text-base-content/60">{invitation.recipient_email}</span
								></td
							><td><span class="badge badge-outline">{invitation.status}</span></td><td
								>{new Date(
									invitation.delivery_attempted_at ?? invitation.created_at
								).toLocaleString()}</td
							><td>{new Date(invitation.expires_at).toLocaleDateString()}</td><td class="flex gap-2"
								>{#if ['pending', 'sent', 'failed'].includes(invitation.status)}<button
										class="btn btn-sm"
										disabled={busyId !== ''}
										onclick={() => resend(invitation)}
										>{busyId === invitation.id ? 'Sending…' : 'Resend'}</button
									><button
										class="btn btn-error btn-sm"
										disabled={busyId !== ''}
										onclick={() => cancel(invitation)}>Cancel</button
									>{:else}—{/if}</td
							></tr
						>
					{:else}<tr><td colspan="5" class="text-center">No invitations.</td></tr>{/each}</tbody
				>
			</table>
		</div>
		<div class="flex items-center justify-between">
			<span>{total} invitations · Page {page} of {Math.max(1, Math.ceil(total / perPage))}</span>
			<div class="flex gap-2">
				<button
					class="btn btn-sm"
					disabled={page <= 1 || busyId !== ''}
					onclick={() => {
						page -= 1;
						void load(page);
					}}>Previous</button
				><button
					class="btn btn-sm"
					disabled={page * perPage >= total || busyId !== ''}
					onclick={() => {
						page += 1;
						void load(page);
					}}>Next</button
				>
			</div>
		</div>
	{/if}
</section>
