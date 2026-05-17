<script lang="ts">
	import { listAdminUsers, type AdminUser } from '$lib/api/admin';

	let {
		placeholder = 'Search users...',
		excludeIds = [],
		onselect = undefined
	}: {
		placeholder?: string;
		excludeIds?: string[];
		onselect?: ((user: AdminUser) => void) | undefined;
	} = $props();

	let query = $state('');
	let results = $state<AdminUser[]>([]);
	let loading = $state(false);
	let open = $state(false);
	let searchTimeout = $state<ReturnType<typeof setTimeout> | undefined>(undefined);

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
		onselect?.(user);
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
			class="input-bordered input w-full pr-8"
			{placeholder}
			value={query}
			on:input={handleInput}
			on:blur={handleBlur}
			on:focus={() => results.length > 0 && (open = true)}
		/>
		{#if loading}
			<span class="loading absolute top-1/2 right-3 loading-xs -translate-y-1/2 loading-spinner"
			></span>
		{/if}
	</div>

	{#if open && results.length > 0}
		<ul
			class="absolute z-50 mt-1 max-h-48 w-full overflow-y-auto rounded-lg border border-base-300 bg-base-100 shadow-lg"
		>
			{#each results as user (user.id)}
				<li>
					<button
						type="button"
						class="flex w-full flex-col px-3 py-2 text-left transition-colors hover:bg-base-200"
						on:mousedown={() => handleSelect(user)}
					>
						<span class="text-sm font-medium">{user.username}</span>
						<span class="text-xs text-base-content/60">{user.email}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>
