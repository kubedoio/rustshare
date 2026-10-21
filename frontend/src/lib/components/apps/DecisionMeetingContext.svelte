<script lang="ts">
	import { decisionsApi, type DecisionMeetingContext } from '$lib/api/decisions';
	import { toastStore } from '$lib/stores/toast';

	let {
		decisionId,
		status = 'proposed',
		decisionDate = null,
		onUpdated
	}: {
		decisionId: string;
		status?: string;
		decisionDate?: string | null;
		onUpdated?: () => void;
	} = $props();
	let meetings = $state<DecisionMeetingContext[]>([]);
	let nextPage = $state<number | null>(null);
	let loading = $state(false);
	let currentStatus = $state('proposed');
	let currentDate = $state('');
	let saving = $state(false);
	let error = $state('');
	let generation = 0;

	$effect(() => {
		if (decisionId) {
			currentStatus = status.toLowerCase() || 'proposed';
			currentDate = (decisionDate ?? '').slice(0, 10);
		}
	});
	$effect(() => {
		const id = decisionId;
		generation += 1;
		meetings = [];
		nextPage = null;
		error = '';
		saving = false;
		void loadMeetings(id, 1, false);
		return () => {
			generation += 1;
		};
	});

	async function loadMeetings(id: string, page: number, append: boolean) {
		const currentGeneration = generation;
		loading = true;
		error = '';
		try {
			const response = await decisionsApi.listMeetings(id, page);
			if (generation !== currentGeneration) return;
			meetings = append
				? Array.from(
						new Map(
							[...meetings, ...response.items].map((meeting) => [meeting.id, meeting])
						).values()
					)
				: response.items;
			nextPage = response.next_page ?? null;
		} catch {
			if (generation === currentGeneration) error = 'Could not load meeting context.';
		} finally {
			if (generation === currentGeneration) loading = false;
		}
	}

	async function save() {
		if (saving) return;
		const currentGeneration = generation;
		saving = true;
		error = '';
		try {
			await decisionsApi.update(decisionId, {
				status: currentStatus,
				...(currentDate && currentDate !== (decisionDate ?? '').slice(0, 10)
					? { decision_date: `${currentDate}T00:00:00Z` }
					: {})
			});
			if (generation !== currentGeneration) return;
			onUpdated?.();
			toastStore.show('Decision updated', 'success');
		} catch {
			if (generation === currentGeneration)
				error = 'Could not update decision. Editing requires permission on this decision.';
		} finally {
			if (generation === currentGeneration) saving = false;
		}
	}
</script>

<section
	class="card-compact card mx-4 mt-3 border border-base-300 bg-base-100 shadow-sm"
	aria-label="Decision context"
>
	<div class="card-body gap-3">
		<h2 class="card-title text-base">Decision context</h2>
		{#if error}<p class="text-sm text-error" role="alert">{error}</p>{/if}
		<form
			class="flex flex-wrap items-end gap-3"
			onsubmit={(event) => {
				event.preventDefault();
				void save();
			}}
		>
			<label class="form-control"
				><span class="label-text">Decision status</span><select
					class="select-bordered select select-sm"
					value={currentStatus}
					onchange={(event) => (currentStatus = event.currentTarget.value)}
					disabled={saving}
					><option value="proposed">Proposed</option><option value="accepted">Accepted</option
					><option value="rejected">Rejected</option><option value="deprecated">Deprecated</option
					><option value="superseded">Superseded</option></select
				></label
			>
			<label class="form-control"
				><span class="label-text">Decision date</span><input
					type="date"
					class="input-bordered input input-sm"
					bind:value={currentDate}
					required={!!decisionDate}
					disabled={saving}
				/></label
			>
			<button class="btn btn-primary btn-sm" disabled={saving}
				>{saving ? 'Saving…' : 'Save decision details'}</button
			>
		</form>
		{#if meetings.length}
			<ul class="divide-y divide-base-300">
				{#each meetings as meeting (meeting.id)}<li class="flex flex-wrap items-center gap-3 py-2">
						<a class="link text-sm link-hover" href={`/apps/meetings/${meeting.id}`}
							>{meeting.is_origin ? 'Origin meeting: ' : 'Meeting: '}{meeting.title}</a
						>{#if meeting.calendar_event_id}<a
								class="link text-xs link-hover"
								href={`/apps/calendar?event_id=${meeting.calendar_event_id}`}>Calendar event</a
							>{/if}
					</li>{/each}
			</ul>
		{/if}
		{#if nextPage !== null}<button
				class="btn self-start btn-ghost btn-xs"
				disabled={loading}
				onclick={() => void loadMeetings(decisionId, nextPage!, true)}
				>Load more meeting context</button
			>{/if}
	</div>
</section>
