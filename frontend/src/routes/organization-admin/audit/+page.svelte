<script lang="ts">
	import { onMount } from 'svelte';
	import { listOrganizationAudit, type OrganizationAuditEvent } from '$lib/api/organization-admin';

	const perPage = 50;
	let entries = $state<OrganizationAuditEvent[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(true);
	let error = $state('');
	let loadVersion = 0;

	async function load(requestedPage = page) {
		const requestVersion = ++loadVersion;
		loading = true;
		error = '';
		try {
			const result = await listOrganizationAudit({ page: requestedPage, per_page: perPage });
			if (requestVersion !== loadVersion || requestedPage !== page) return;
			entries = result.entries ?? [];
			total = result.total;
		} catch (e) {
			if (requestVersion !== loadVersion || requestedPage !== page) return;
			error = e instanceof Error ? e.message : 'Unable to load audit activity';
		} finally {
			if (requestVersion === loadVersion && requestedPage === page) loading = false;
		}
	}

	onMount(() => {
		void load();
	});
</script>

<svelte:head><title>Audit — Organization administration</title></svelte:head>
<section class="space-y-4">
	<h2 class="text-2xl font-semibold">Organization audit</h2>
	{#if error}<div class="alert alert-error" role="alert">{error}</div>
	{:else if loading}<p role="status" aria-live="polite">Loading audit activity…</p>
	{:else}
		<div class="overflow-x-auto rounded-lg border border-base-300">
			<table class="table table-zebra">
				<caption class="sr-only">Organization administrative audit events</caption>
				<thead><tr><th>Time</th><th>Action</th><th>Actor</th><th>Target</th></tr></thead>
				<tbody
					>{#each entries as entry (entry.id)}
						<tr
							><td>{new Date(entry.occurred_at).toLocaleString()}</td><td>{entry.action_type}</td
							><td>{entry.actor_id ?? 'System'}</td><td
								>{entry.target_type ?? '—'}{entry.target_id ? ` · ${entry.target_id}` : ''}</td
							></tr
						>
					{:else}<tr><td colspan="4" class="text-center">No audit events.</td></tr>{/each}</tbody
				>
			</table>
		</div>
		<div class="flex items-center justify-between">
			<span>{total} events · Page {page} of {Math.max(1, Math.ceil(total / perPage))}</span>
			<div class="flex gap-2">
				<button
					class="btn btn-sm"
					disabled={page <= 1}
					onclick={() => {
						page -= 1;
						void load(page);
					}}>Previous</button
				><button
					class="btn btn-sm"
					disabled={page * perPage >= total}
					onclick={() => {
						page += 1;
						void load(page);
					}}>Next</button
				>
			</div>
		</div>
	{/if}
</section>
