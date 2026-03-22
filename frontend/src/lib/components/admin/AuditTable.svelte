<script lang="ts">
	import type { AuditEntry } from '$lib/api/admin';

	export let entries: AuditEntry[] = [];
	export let total: number = 0;
	export let page: number = 1;
	export let perPage: number = 50;
	export let onPageChange: (page: number) => void = () => {};
	export let onFilterChange: (filters: {
		type?: string;
		user_id?: string;
		from?: string;
		to?: string;
	}) => void = () => {};

	let typeFilter = '';
	let userSearch = '';
	let fromDate = '';
	let toDate = '';
	let expandedId: string | null = null;

	$: totalPages = Math.ceil(total / perPage);

	function applyFilters() {
		onFilterChange({
			type: typeFilter || undefined,
			user_id: userSearch.trim() || undefined,
			from: fromDate || undefined,
			to: toDate || undefined
		});
	}

	function toggleExpand(id: string) {
		expandedId = expandedId === id ? null : id;
	}

	function formatDate(dateStr: string) {
		return new Date(dateStr).toLocaleString();
	}

	const ACTION_COLORS: Record<string, string> = {
		create: 'badge-success',
		update: 'badge-info',
		delete: 'badge-error',
		login: 'badge-primary',
		logout: 'badge-ghost',
		disable: 'badge-warning',
		enable: 'badge-success'
	};

	function actionColor(action: string): string {
		return ACTION_COLORS[action.toLowerCase()] ?? 'badge-ghost';
	}
</script>

<div class="space-y-4">
	<!-- Filter bar -->
	<div class="card bg-base-100 shadow">
		<div class="card-body p-4">
			<div class="flex flex-wrap gap-3 items-end">
				<div class="form-control">
					<label class="label py-0" for="audit-type"><span class="label-text text-xs">Type</span></label>
					<select
						id="audit-type"
						class="select select-bordered select-sm w-40"
						bind:value={typeFilter}
						on:change={applyFilters}
					>
						<option value="">All</option>
						<option value="share_access">Share Access</option>
						<option value="security_event">Security Events</option>
						<option value="admin_action">Admin Actions</option>
					</select>
				</div>

				<div class="form-control">
					<label class="label py-0" for="audit-user">
						<span class="label-text text-xs">User ID / search</span>
					</label>
					<input
						id="audit-user"
						type="text"
						class="input input-bordered input-sm w-44"
						bind:value={userSearch}
						placeholder="Filter by user..."
						on:input={applyFilters}
					/>
				</div>

				<div class="form-control">
					<label class="label py-0" for="audit-from"><span class="label-text text-xs">From</span></label>
					<input
						id="audit-from"
						type="date"
						class="input input-bordered input-sm"
						bind:value={fromDate}
						on:change={applyFilters}
					/>
				</div>

				<div class="form-control">
					<label class="label py-0" for="audit-to"><span class="label-text text-xs">To</span></label>
					<input
						id="audit-to"
						type="date"
						class="input input-bordered input-sm"
						bind:value={toDate}
						on:change={applyFilters}
					/>
				</div>

				<span class="text-sm text-base-content/60 ml-auto self-center">
					{total} event{total !== 1 ? 's' : ''}
				</span>
			</div>
		</div>
	</div>

	<!-- Table -->
	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table table-zebra w-full">
			<thead>
				<tr>
					<th>Timestamp</th>
					<th>Actor</th>
					<th>Type</th>
					<th>Action</th>
					<th>Target</th>
					<th>Detail</th>
				</tr>
			</thead>
			<tbody>
				{#each entries as entry (entry.id)}
					<tr>
						<td class="text-xs text-base-content/70 whitespace-nowrap">
							{formatDate(entry.occurred_at)}
						</td>
						<td class="text-sm font-medium">{entry.actor_label}</td>
						<td>
							<span class="badge badge-ghost badge-sm capitalize">{entry.type}</span>
						</td>
						<td>
							<span class="badge badge-sm {actionColor(entry.action_type)} capitalize">
								{entry.action_type}
							</span>
						</td>
						<td class="text-sm text-base-content/70">{entry.target_label ?? '—'}</td>
						<td>
							{#if Object.keys(entry.detail).length > 0}
								<button
									class="btn btn-ghost btn-xs"
									on:click={() => toggleExpand(entry.id)}
								>
									{expandedId === entry.id ? 'Collapse' : 'Expand'}
								</button>
							{:else}
								<span class="text-base-content/40 text-xs">—</span>
							{/if}
						</td>
					</tr>
					{#if expandedId === entry.id}
						<tr>
							<td colspan="6" class="bg-base-200">
								<pre class="text-xs p-2 overflow-x-auto rounded">{JSON.stringify(entry.detail, null, 2)}</pre>
							</td>
						</tr>
					{/if}
				{/each}
				{#if entries.length === 0}
					<tr>
						<td colspan="6" class="text-center text-base-content/50 py-8">No audit events found</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	<!-- Pagination -->
	{#if totalPages > 1}
		<div class="flex justify-center gap-2">
			<button
				class="btn btn-sm btn-ghost"
				disabled={page <= 1}
				on:click={() => onPageChange(page - 1)}
			>
				Previous
			</button>
			<span class="flex items-center px-2 text-sm">Page {page} of {totalPages}</span>
			<button
				class="btn btn-sm btn-ghost"
				disabled={page >= totalPages}
				on:click={() => onPageChange(page + 1)}
			>
				Next
			</button>
		</div>
	{/if}
</div>
