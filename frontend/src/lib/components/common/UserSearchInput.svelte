<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { listAdminUsers, type AdminUser } from '$lib/api/admin';

	export let placeholder = 'Search users...';
	export let excludeIds: string[] = [];

	const dispatch = createEventDispatcher<{ select: AdminUser }>();

	let query = '';
	let results: AdminUser[] = [];
	let loading = false;
	let open = false;
	let searchTimeout: ReturnType<typeof setTimeout>;

	async function search(q: string) {
		if (!q.trim()) {
			results = [];
			open = false;
			return;
		}
		loading = true;
		try {
			const res = await listAdminUsers({ search: q, per_page: 10 });
			results = res.users.filter((u) => !excludeIds.includes(u.id));
			open = results.length > 0;
		} catch {
			results = [];
		} finally {
			loading = false;
		}
	}

	function handleInput(e: Event) {
		query = (e.target as HTMLInputElement).value;
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(() => search(query), 300);
	}

	function handleSelect(user: AdminUser) {
		dispatch('select', user);
		query = '';
		results = [];
		open = false;
	}

	function handleBlur() {
		// Delay to allow click on dropdown
		setTimeout(() => {
			open = false;
		}, 200);
	}
</script>

<div class="relative">
	<div class="relative">
		<input
			type="text"
			class="input input-bordered w-full pr-8"
			{placeholder}
			value={query}
			on:input={handleInput}
			on:blur={handleBlur}
			on:focus={() => results.length > 0 && (open = true)}
		/>
		{#if loading}
			<span class="loading loading-spinner loading-xs absolute right-3 top-1/2 -translate-y-1/2"></span>
		{/if}
	</div>

	{#if open && results.length > 0}
		<ul class="absolute z-50 mt-1 w-full bg-base-100 border border-base-300 rounded-lg shadow-lg max-h-48 overflow-y-auto">
			{#each results as user (user.id)}
				<li>
					<button
						type="button"
						class="w-full text-left px-3 py-2 hover:bg-base-200 transition-colors flex flex-col"
						on:mousedown={() => handleSelect(user)}
					>
						<span class="font-medium text-sm">{user.username}</span>
						<span class="text-xs text-base-content/60">{user.email}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>
