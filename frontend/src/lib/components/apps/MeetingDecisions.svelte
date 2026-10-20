<script lang="ts">
	import {
		meetingsApi,
		type CreateMeetingDecision,
		type MeetingDecisionLink,
		type MeetingResourceCandidate
	} from '$lib/api/meetings';
	import { toastStore } from '$lib/stores/toast';

	let { meetingId }: { meetingId: string } = $props();
	let links = $state<MeetingDecisionLink[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(false);
	let working = $state(false);
	let error = $state('');
	let showCreate = $state(false);
	let showPicker = $state(false);
	let title = $state('');
	let category = $state('General');
	let content = $state('');
	let requestId = $state('');
	let retrying = $state(false);
	let pendingRequest = $state<CreateMeetingDecision | null>(null);
	let query = $state('');
	let searching = $state(false);
	let candidates = $state<MeetingResourceCandidate[]>([]);
	let generation = 0;
	let searchGeneration = 0;

	function storageKey(id: string) {
		return `meeting-decision-request:${id}`;
	}
	function readRequestId(id: string) {
		try {
			return sessionStorage.getItem(storageKey(id)) ?? '';
		} catch {
			return '';
		}
	}
	function rememberRequestId(id: string, value: string) {
		try {
			if (value) sessionStorage.setItem(storageKey(id), value);
			else sessionStorage.removeItem(storageKey(id));
		} catch {
			/* Retries remain safe while this component stays open. */
		}
	}

	$effect(() => {
		const id = meetingId;
		generation += 1;
		searchGeneration += 1;
		links = [];
		total = 0;
		page = 1;
		error = '';
		working = false;
		showCreate = false;
		showPicker = false;
		title = '';
		category = 'General';
		content = '';
		query = '';
		searching = false;
		candidates = [];
		const savedRequestId = readRequestId(id);
		requestId = savedRequestId;
		retrying = !!savedRequestId;
		pendingRequest = null;
		void loadPage(id, 1, false);
		return () => {
			generation += 1;
		};
	});

	async function loadPage(id: string, requestedPage: number, append: boolean) {
		const currentGeneration = generation;
		loading = true;
		try {
			const response = await meetingsApi.listDecisions(id, requestedPage);
			if (currentGeneration !== generation) return;
			links = append ? [...links, ...response.items] : response.items;
			total = response.total;
			page = response.page;
		} catch {
			if (currentGeneration === generation) error = 'Could not load meeting decisions.';
		} finally {
			if (currentGeneration === generation) loading = false;
		}
	}

	function newDraft() {
		rememberRequestId(meetingId, '');
		requestId = '';
		pendingRequest = null;
		retrying = false;
		title = '';
		category = 'General';
		content = '';
		error = '';
	}

	async function createDecision() {
		if (working || !title.trim()) return;
		const id = meetingId;
		const currentGeneration = generation;
		if (!requestId) requestId = crypto.randomUUID();
		pendingRequest ??= {
			request_id: requestId,
			title: title.trim(),
			category: category.trim() || 'General',
			content
		};
		rememberRequestId(id, requestId);
		working = true;
		error = '';
		try {
			await meetingsApi.createDecision(id, pendingRequest);
			rememberRequestId(id, '');
			if (currentGeneration !== generation) return;
			newDraft();
			showCreate = false;
			await loadPage(id, 1, false);
			toastStore.show('Decision created', 'success');
		} catch {
			if (currentGeneration === generation) {
				retrying = true;
				error = 'Could not confirm creation. Retry the same details to recover the result.';
			}
		} finally {
			if (currentGeneration === generation) working = false;
		}
	}

	function decisionId(candidate: MeetingResourceCandidate): string | undefined {
		return /^elembra:\/\/io\.elembra\.files\/file\/([0-9a-f-]{36})$/i.exec(
			candidate.resource_ref
		)?.[1];
	}

	async function search() {
		if (!query.trim()) return;
		const currentGeneration = generation;
		const currentSearch = ++searchGeneration;
		searching = true;
		error = '';
		try {
			const results = await meetingsApi.searchResources(query.trim());
			if (currentGeneration !== generation || currentSearch !== searchGeneration) return;
			candidates = results.filter(
				(candidate) =>
					decisionId(candidate) &&
					/(?:^|\/)DEC-[^/]+\.md$/i.test(candidate.location ?? candidate.title)
			);
		} catch {
			if (currentGeneration === generation && currentSearch === searchGeneration) {
				candidates = [];
				error = 'Could not search accessible decisions.';
			}
		} finally {
			if (currentGeneration === generation && currentSearch === searchGeneration) searching = false;
		}
	}

	async function link(candidate: MeetingResourceCandidate) {
		const targetId = decisionId(candidate);
		if (working || !targetId) return;
		const id = meetingId;
		const currentGeneration = generation;
		working = true;
		error = '';
		try {
			await meetingsApi.linkDecision(id, targetId);
			if (currentGeneration !== generation) return;
			await loadPage(id, 1, false);
		} catch {
			if (currentGeneration === generation) error = 'Could not link this decision.';
		} finally {
			if (currentGeneration === generation) working = false;
		}
	}

	async function unlink(linkId: string) {
		if (working) return;
		const currentGeneration = generation;
		working = true;
		error = '';
		try {
			await meetingsApi.unlinkDecision(meetingId, linkId);
			if (currentGeneration !== generation) return;
			links = links.filter((link) => link.id !== linkId);
			total = Math.max(0, total - 1);
		} catch {
			if (currentGeneration === generation) error = 'Could not remove the meeting link.';
		} finally {
			if (currentGeneration === generation) working = false;
		}
	}
</script>

<section
	class="card-compact card mx-4 mt-3 border border-base-300 bg-base-100 shadow-sm"
	aria-label="Meeting decisions"
>
	<div class="card-body gap-3">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<div>
				<h2 class="card-title text-base">Decisions</h2>
				<p class="text-xs text-base-content/60">
					Decisions keep their own permissions. Removing a link preserves the decision.
				</p>
			</div>
			<div class="flex gap-2">
				<button
					class="btn btn-outline btn-sm"
					disabled={working}
					onclick={() => {
						showCreate = !showCreate;
						showPicker = false;
					}}>Create Decision</button
				>
				<button
					class="btn btn-outline btn-sm"
					disabled={working}
					onclick={() => {
						showPicker = !showPicker;
						showCreate = false;
					}}>Link Decision</button
				>
			</div>
		</div>
		{#if error}<p class="text-sm text-error" role="alert">{error}</p>{/if}
		{#if showCreate}
			<form
				class="grid gap-2"
				onsubmit={(event) => {
					event.preventDefault();
					void createDecision();
				}}
			>
				{#if retrying}<p class="text-xs text-base-content/60">
						A previous attempt may have completed. Retry the same details to recover it, or start a
						new draft for a separate decision.
					</p>{/if}
				<label class="form-control"
					><span class="label-text">Decision title</span><input
						class="input-bordered input input-sm"
						bind:value={title}
						required
						disabled={working || (retrying && pendingRequest !== null)}
					/></label
				>
				<label class="form-control"
					><span class="label-text">Category</span><input
						class="input-bordered input input-sm"
						bind:value={category}
						disabled={working || (retrying && pendingRequest !== null)}
					/></label
				>
				<label class="form-control"
					><span class="label-text">Decision content</span><textarea
						class="textarea-bordered textarea"
						bind:value={content}
						disabled={working || (retrying && pendingRequest !== null)}></textarea></label
				>
				<div class="flex gap-2">
					<button class="btn btn-primary btn-sm" disabled={working || !title.trim()}
						>{working ? 'Creating…' : retrying ? 'Retry creation' : 'Create decision'}</button
					>{#if retrying}<button
							type="button"
							class="btn btn-ghost btn-sm"
							disabled={working}
							onclick={newDraft}>Start a new draft</button
						>{/if}
				</div>
			</form>
		{/if}
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
					aria-label="Search accessible Decisions"
					placeholder="Search accessible Decisions"
					bind:value={query}
				/>
				<button class="btn btn-primary btn-sm" disabled={!query.trim()}>Search</button>
			</form>
			<ul class="max-h-48 divide-y divide-base-300 overflow-y-auto">
				{#each candidates as candidate (candidate.resource_ref)}<li
						class="flex items-center justify-between gap-2 py-2"
					>
						<span class="truncate text-sm">{candidate.title}</span><button
							class="btn btn-ghost btn-xs"
							disabled={working}
							onclick={() => void link(candidate)}>Link</button
						>
					</li>{/each}
			</ul>
		{/if}
		{#if loading && !links.length}<p class="text-sm text-base-content/60">Loading decisions…</p>
		{:else if links.length}
			<ul class="divide-y divide-base-300">
				{#each links as link (link.id)}
					<li class="flex items-center justify-between gap-3 py-2">
						{#if link.available && link.decision}
							<div class="min-w-0 flex-1">
								<a class="link text-sm link-hover" href={link.decision.open_url}
									>{link.decision.title || link.decision.name}</a
								>
								<p class="text-xs text-base-content/60">
									{link.decision.status || 'Unspecified'}{#if link.decision.decision_date}
										· <time datetime={link.decision.decision_date}
											>{link.decision.decision_date.slice(0, 10)}</time
										>{/if}
									{#if link.decision.owner_display_name}
										· Owner: {link.decision.owner_display_name}{/if}
								</p>
							</div>
						{:else}<span class="text-sm text-base-content/55">Linked decision unavailable</span
							>{/if}
						<button
							class="btn btn-ghost btn-xs"
							aria-label="Remove decision link"
							disabled={working}
							onclick={() => void unlink(link.id)}>Remove</button
						>
					</li>
				{/each}
			</ul>
		{:else if !loading}<p class="text-sm text-base-content/60">No linked decisions yet.</p>{/if}
		{#if total > links.length}<button
				class="btn self-start btn-ghost btn-xs"
				disabled={loading}
				onclick={() => void loadPage(meetingId, page + 1, true)}>Load more decisions</button
			>{/if}
	</div>
</section>
