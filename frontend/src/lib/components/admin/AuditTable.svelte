<script lang="ts">
	import type { AuditEntry } from '$lib/api/admin';

	let {
		entries = [],
		total = 0,
		page = 1,
		perPage = 50,
		onPageChange = () => {},
		onFilterChange = () => {}
	}: {
		entries?: AuditEntry[];
		total?: number;
		page?: number;
		perPage?: number;
		onPageChange?: (page: number) => void;
		onFilterChange?: (filters: {
			type?: string;
			user_id?: string;
			from?: string;
			to?: string;
		}) => void;
	} = $props();

	let typeFilter = $state('');
	let userSearch = $state('');
	let fromDate = $state('');
	let toDate = $state('');
	let expandedId = $state<string | null>(null);

	let totalPages = $derived(Math.ceil(total / perPage));

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
			<div class="flex flex-wrap items-end gap-3">
				<div class="form-control">
					<label class="label py-0" for="audit-type"
						><span class="label-text text-xs">Type</span></label
					>
					<select
						id="audit-type"
						class="select-bordered select w-40 select-sm"
						bind:value={typeFilter}
						onchange={applyFilters}
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
						class="input-bordered input input-sm w-44"
						bind:value={userSearch}
						placeholder="Filter by user..."
						oninput={applyFilters}
					/>
				</div>

				<div class="form-control">
					<label class="label py-0" for="audit-from"
						><span class="label-text text-xs">From</span></label
					>
					<input
						id="audit-from"
						type="date"
						class="input-bordered input input-sm"
						bind:value={fromDate}
						onchange={applyFilters}
					/>
				</div>

				<div class="form-control">
					<label class="label py-0" for="audit-to"><span class="label-text text-xs">To</span></label
					>
					<input
						id="audit-to"
						type="date"
						class="input-bordered input input-sm"
						bind:value={toDate}
						onchange={applyFilters}
					/>
				</div>

				<span class="ml-auto self-center text-sm text-base-content/60">
					{total} event{total !== 1 ? 's' : ''}
				</span>
			</div>
		</div>
	</div>

	<!-- Table -->
	<div class="overflow-x-auto rounded-lg border border-base-300">
		<table class="table w-full table-zebra">
			<thead>
				<tr>
					<th class="font-data">Timestamp</th>
					<th class="font-data">Actor</th>
					<th class="font-data">Type</th>
					<th class="font-data">Action</th>
					<th class="font-data">Target</th>
					<th class="font-data">Detail</th>
				</tr>
			</thead>
			<tbody>
				{#each entries as entry (entry.id)}
					<tr>
						<td class="font-data text-xs whitespace-nowrap text-base-content/70 tabular-nums">
							{formatDate(entry.occurred_at)}
						</td>
						<td class="font-data text-sm font-medium">{entry.actor_label}</td>
						<td>
							<span class="badge badge-ghost badge-sm capitalize">{entry.type}</span>
						</td>
						<td>
							<span class="badge badge-sm {actionColor(entry.action_type)} capitalize">
								{entry.action_type}
							</span>
						</td>
						<td class="font-data text-sm text-base-content/70">{entry.target_label ?? '—'}</td>
						<td>
							{#if Object.keys(entry.detail).length > 0}
								<button class="btn btn-ghost btn-xs" onclick={() => toggleExpand(entry.id)}>
									{expandedId === entry.id ? 'Collapse' : 'Expand'}
								</button>
							{:else}
								<span class="text-xs text-base-content/40">—</span>
							{/if}
						</td>
					</tr>
					{#if expandedId === entry.id}
						<tr>
							<td colspan="6" class="bg-base-200">
								<pre class="overflow-x-auto rounded p-2 text-xs">{JSON.stringify(
										entry.detail,
										null,
										2
									)}</pre>
							</td>
						</tr>
					{/if}
				{/each}
				{#if entries.length === 0}
					<tr>
						<td colspan="6" class="py-8 text-center text-base-content/50">No audit events found</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	<!-- Pagination -->
	{#if totalPages > 1}
		<div class="flex justify-center gap-2">
			<button
				class="btn btn-ghost btn-sm"
				disabled={page <= 1}
				onclick={() => onPageChange(page - 1)}
			>
				Previous
			</button>
			<span class="flex items-center px-2 text-sm">Page {page} of {totalPages}</span>
			<button
				class="btn btn-ghost btn-sm"
				disabled={page >= totalPages}
				onclick={() => onPageChange(page + 1)}
			>
				Next
			</button>
		</div>
	{/if}
</div>
