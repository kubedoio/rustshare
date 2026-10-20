<script lang="ts">
	import {
		meetingsApi,
		type MeetingResourceCandidate,
		type MeetingResourceLink
	} from '$lib/api/meetings';
	import { toastStore } from '$lib/stores/toast';

	let { meetingId }: { meetingId: string } = $props();
	let links = $state<MeetingResourceLink[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(false);
	let working = $state<string | null>(null);
	let showPicker = $state(false);
	let query = $state('');
	let searching = $state(false);
	let results = $state<MeetingResourceCandidate[]>([]);
	let searchError = $state('');

	$effect(() => {
		let cancelled = false;
		const currentMeetingId = meetingId;
		links = [];
		total = 0;
		page = 1;
		results = [];
		query = '';
		void loadPage(currentMeetingId, 1, false, () => cancelled);
		return () => {
			cancelled = true;
		};
	});

	async function loadPage(
		id: string,
		requestedPage: number,
		append: boolean,
		isCancelled = () => false
	) {
		loading = true;
		try {
			const response = await meetingsApi.listResources(id, requestedPage);
			if (isCancelled()) return;
			links = append ? [...links, ...response.items] : response.items;
			total = response.total;
			page = response.page;
		} catch {
			if (!isCancelled()) toastStore.show('Could not load linked resources', 'error');
		} finally {
			if (!isCancelled()) loading = false;
		}
	}

	async function search() {
		if (!query.trim()) return;
		searching = true;
		searchError = '';
		try {
			results = await meetingsApi.searchResources(query.trim());
		} catch (error) {
			results = [];
			searchError = error instanceof Error ? error.message : 'Search failed';
		} finally {
			searching = false;
		}
	}

	async function linkResource(candidate: MeetingResourceCandidate) {
		if (working) return;
		working = candidate.resource_ref;
		try {
			await meetingsApi.linkResource(meetingId, candidate.resource_ref);
			await loadPage(meetingId, 1, false);
			toastStore.show('Resource linked', 'success');
		} catch (error) {
			toastStore.show(error instanceof Error ? error.message : 'Could not link resource', 'error');
		} finally {
			working = null;
		}
	}

	async function unlinkResource(link: MeetingResourceLink) {
		if (working) return;
		working = link.id;
		try {
			await meetingsApi.unlinkResource(meetingId, link.id);
			links = links.filter((item) => item.id !== link.id);
			total = Math.max(0, total - 1);
			toastStore.show('Meeting link removed', 'success');
		} catch {
			toastStore.show('Could not remove meeting link', 'error');
		} finally {
			working = null;
		}
	}
</script>

<section
	class="card-compact card mx-4 mt-3 border border-base-300 bg-base-100 shadow-sm"
	aria-label="Linked resources"
>
	<div class="card-body gap-2">
		<div class="flex items-center justify-between gap-3">
			<div>
				<h2 class="card-title text-base">Linked resources</h2>
				<p class="text-xs text-base-content/60">
					Files and Notes stay in their original location and keep their own permissions.
				</p>
			</div>
			<button class="btn btn-outline btn-sm" onclick={() => (showPicker = !showPicker)}>
				{showPicker ? 'Close' : 'Link a resource'}
			</button>
		</div>

		{#if showPicker}
			<form
				class="flex gap-2"
				onsubmit={(event) => {
					event.preventDefault();
					void search();
				}}
			>
				<input
					class="input-bordered input min-w-0 flex-1 input-sm"
					aria-label="Search accessible Files and Notes"
					placeholder="Search accessible Files and Notes"
					bind:value={query}
				/>
				<button class="btn btn-primary btn-sm" disabled={searching || !query.trim()}>
					{searching ? 'Searching…' : 'Search'}
				</button>
			</form>
			{#if searchError}<p role="alert" class="text-sm text-error">{searchError}</p>{/if}
			{#if results.length}
				<ul
					class="max-h-48 divide-y divide-base-300 overflow-y-auto rounded-box border border-base-300"
				>
					{#each results as candidate (candidate.resource_ref)}
						<li class="flex items-center justify-between gap-3 p-2">
							<div class="min-w-0">
								<p class="truncate text-sm font-medium">{candidate.title}</p>
								<p class="truncate text-xs text-base-content/60">
									{candidate.location ?? candidate.source_type}
								</p>
							</div>
							<button
								class="btn btn-ghost btn-xs"
								disabled={working !== null}
								onclick={() => void linkResource(candidate)}
							>
								{working === candidate.resource_ref ? 'Linking…' : 'Link'}
							</button>
						</li>
					{/each}
				</ul>
			{:else if query.trim() && !searching && !searchError}
				<p class="text-sm text-base-content/60">No accessible Files or Notes found.</p>
			{/if}
		{/if}

		{#if loading && !links.length}
			<p class="text-sm text-base-content/60">Loading linked resources…</p>
		{:else if links.length}
			<ul class="max-h-44 divide-y divide-base-300 overflow-y-auto">
				{#each links as link (link.id)}
					<li class="flex items-center justify-between gap-3 py-2">
						{#if link.available && link.open_url}
							<a class="min-w-0 flex-1 link truncate text-sm link-hover" href={link.open_url}>
								{link.display_name}
								<span class="ml-1 text-xs text-base-content/55">{link.resource_kind}</span>
							</a>
						{:else}
							<span class="min-w-0 flex-1 truncate text-sm text-base-content/55"
								>Linked resource unavailable</span
							>
						{/if}
						<button
							class="btn btn-ghost btn-xs"
							aria-label="Remove linked resource"
							disabled={working !== null}
							onclick={() => void unlinkResource(link)}
						>
							{working === link.id ? 'Removing…' : 'Remove'}
						</button>
					</li>
				{/each}
			</ul>
		{:else if !loading}
			<p class="text-sm text-base-content/60">No linked resources yet.</p>
		{/if}
		{#if total > links.length}
			<button
				class="btn self-start btn-ghost btn-xs"
				disabled={loading}
				onclick={() => void loadPage(meetingId, page + 1, true)}
			>
				Load more
			</button>
		{/if}
	</div>
</section>
