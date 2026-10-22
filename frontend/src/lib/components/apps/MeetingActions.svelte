<script lang="ts">
	import {
		meetingsApi,
		type CreateMeetingAction,
		type MeetingAction,
		type MeetingActionPerson,
		type MeetingDecisionLink,
		type UpdateMeetingAction
	} from '$lib/api/meetings';
	import { ApiError } from '$lib/api/types';
	import { currentUser } from '$lib/stores/auth';
	import { toastStore } from '$lib/stores/toast';

	let { meetingId }: { meetingId: string } = $props();
	let actions = $state<MeetingAction[]>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(false);
	let working = $state(false);
	let error = $state('');
	let showForm = $state(false);
	let editing = $state<MeetingAction | null>(null);
	let title = $state('');
	let description = $state('');
	let assigneeId = $state('');
	let dueDate = $state('');
	let decisionId = $state('');
	let people = $state<MeetingActionPerson[]>([]);
	let peopleTotal = $state(0);
	let peoplePage = $state(0);
	let decisions = $state<MeetingDecisionLink[]>([]);
	let decisionsTotal = $state(0);
	let decisionsPage = $state(0);
	let loadingOptions = $state(false);
	let pendingCreate = $state<CreateMeetingAction | null>(null);
	let pendingUpdate = $state<{ id: string; request: UpdateMeetingAction } | null>(null);
	let generation = 0;
	let listGeneration = 0;
	let optionsGeneration = 0;
	type Principal = { id: string; tenant_id?: string };

	function draftKey(principal: Principal, id: string) {
		return `meeting-action-request:${principal.tenant_id ?? ''}:${principal.id}:${id}`;
	}

	function remember(principal: Principal | null, id: string, request: CreateMeetingAction | null) {
		if (!principal) return;
		try {
			if (request) sessionStorage.setItem(draftKey(principal, id), JSON.stringify(request));
			else sessionStorage.removeItem(draftKey(principal, id));
		} catch {
			/* The in-memory request still protects retries while this view stays open. */
		}
	}

	function restore(principal: Principal | null, id: string): CreateMeetingAction | null {
		if (!principal) return null;
		try {
			const stored = sessionStorage.getItem(draftKey(principal, id));
			if (!stored) return null;
			const value = JSON.parse(stored) as CreateMeetingAction;
			return value &&
				typeof value.request_id === 'string' &&
				typeof value.title === 'string' &&
				['description', 'assignee_id', 'due_date', 'related_decision_id'].every(
					(key) =>
						value[key as keyof CreateMeetingAction] === null ||
						typeof value[key as keyof CreateMeetingAction] === 'string'
				)
				? value
				: null;
		} catch {
			return null;
		}
	}

	function forgetCompleted(principal: Principal | null, id: string, requestId: string) {
		if (restore(principal, id)?.request_id === requestId) remember(principal, id, null);
	}

	function fillForm(value: {
		title: string;
		description: string | null;
		assignee_id: string | null;
		due_date: string | null;
		related_decision_id?: string | null;
	}) {
		title = value.title;
		description = value.description ?? '';
		assigneeId = value.assignee_id ?? '';
		dueDate = value.due_date ?? '';
		decisionId = value.related_decision_id ?? '';
	}

	$effect(() => {
		const id = meetingId;
		const principal = $currentUser;
		generation += 1;
		actions = [];
		total = 0;
		page = 1;
		error = '';
		working = false;
		showForm = false;
		editing = null;
		pendingUpdate = null;
		people = [];
		peopleTotal = 0;
		peoplePage = 0;
		decisions = [];
		decisionsTotal = 0;
		decisionsPage = 0;
		loadingOptions = false;
		const saved = restore(principal, id);
		pendingCreate = saved;
		fillForm(saved ?? { title: '', description: null, assignee_id: null, due_date: null });
		void loadPage(id, 1, false);
		return () => {
			generation += 1;
		};
	});

	async function loadPage(id: string, requestedPage: number, append: boolean) {
		const current = generation;
		const request = ++listGeneration;
		loading = true;
		try {
			const result = await meetingsApi.listActions(id, requestedPage);
			if (current !== generation || request !== listGeneration) return;
			actions = append ? [...actions, ...result.items] : result.items;
			total = result.total;
			page = result.page;
		} catch {
			if (current === generation && request === listGeneration) {
				actions = [];
				total = 0;
				error = 'Could not load meeting actions.';
			}
		} finally {
			if (current === generation && request === listGeneration) loading = false;
		}
	}

	async function loadOptions(kind: 'people' | 'decisions') {
		const current = generation;
		const options = optionsGeneration;
		const id = meetingId;
		if (loadingOptions) return;
		loadingOptions = true;
		try {
			if (kind === 'people') {
				const result = await meetingsApi.listActionAssignees(id, peoplePage + 1);
				if (current !== generation || options !== optionsGeneration) return;
				people = [...people, ...result.items];
				peopleTotal = result.total;
				peoplePage = result.page;
			} else {
				const result = await meetingsApi.listDecisions(id, decisionsPage + 1);
				if (current !== generation || options !== optionsGeneration) return;
				decisions = [...decisions, ...result.items];
				decisionsTotal = result.total;
				decisionsPage = result.page;
			}
		} catch {
			if (current === generation && options === optionsGeneration)
				error = 'Could not load action choices. Try again.';
		} finally {
			if (current === generation && options === optionsGeneration) loadingOptions = false;
		}
	}

	async function openForm(action: MeetingAction | null) {
		optionsGeneration += 1;
		const options = optionsGeneration;
		people = [];
		peoplePage = 0;
		peopleTotal = 0;
		decisions = [];
		decisionsPage = 0;
		decisionsTotal = 0;
		loadingOptions = false;
		editing = action;
		error = '';
		fillForm(
			action
				? { ...action, related_decision_id: action.related_decision?.id }
				: (pendingCreate ?? { title: '', description: null, assignee_id: null, due_date: null })
		);
		showForm = true;
		const current = generation;
		if (!peoplePage) await loadOptions('people');
		if (current === generation && options === optionsGeneration) await loadOptions('decisions');
	}

	async function save() {
		if (working || !title.trim()) return;
		if (new TextEncoder().encode(description).length > 16384) {
			error = 'Action description must be at most 16 KiB.';
			return;
		}
		if (editing && !pendingUpdate) {
			pendingUpdate = {
				id: editing.id,
				request: {
					request_id: crypto.randomUUID(),
					expected_version: editing.version,
					title: title.trim(),
					description: description || null,
					assignee_id: assigneeId || null,
					due_date: dueDate || null,
					state: editing.state,
					...(decisionId === (editing.related_decision?.id ?? '')
						? {}
						: { related_decision_id: decisionId || null })
				}
			};
		}
		if (editing) {
			await update();
			return;
		}
		const id = meetingId;
		const principal = $currentUser;
		const current = generation;
		pendingCreate ??= {
			request_id: crypto.randomUUID(),
			title: title.trim(),
			description: description || null,
			assignee_id: assigneeId || null,
			due_date: dueDate || null,
			related_decision_id: decisionId || null
		};
		remember(principal, id, pendingCreate);
		const submitted = pendingCreate;
		working = true;
		error = '';
		try {
			await meetingsApi.createAction(id, submitted);
			forgetCompleted(principal, id, submitted.request_id);
			if (current !== generation) return;
			pendingCreate = null;
			showForm = false;
			await loadPage(id, 1, false);
			if (current === generation) toastStore.show('Action created', 'success');
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 400) {
				forgetCompleted(principal, id, submitted.request_id);
				if (current !== generation) return;
				pendingCreate = null;
				error = 'Action details were rejected. Check the details and try again.';
			} else if (current === generation)
				error = 'Could not confirm creation. Retry the same details to recover the result.';
		} finally {
			if (current === generation) working = false;
		}
	}

	async function toggle(action: MeetingAction) {
		if (working || pendingUpdate) return;
		pendingUpdate = {
			id: action.id,
			request: {
				request_id: crypto.randomUUID(),
				expected_version: action.version,
				title: action.title,
				description: action.description,
				assignee_id: action.assignee_id,
				due_date: action.due_date,
				state: action.state === 'open' ? 'completed' : 'open'
			}
		};
		await update();
	}

	async function update() {
		if (working || !pendingUpdate) return;
		const id = meetingId;
		const current = generation;
		const pending = pendingUpdate;
		working = true;
		error = '';
		try {
			await meetingsApi.updateAction(id, pending.id, pending.request);
			if (current !== generation) return;
			pendingUpdate = null;
			showForm = false;
			editing = null;
			await loadPage(id, 1, false);
		} catch (cause) {
			if (current !== generation) return;
			if (cause instanceof ApiError && cause.status === 400) {
				pendingUpdate = null;
				error = 'Action details were rejected. Check the details and try again.';
			} else if (cause instanceof ApiError && cause.status === 409) {
				pendingUpdate = null;
				showForm = false;
				editing = null;
				await loadPage(id, 1, false);
				if (current === generation)
					error = 'This action changed. Review the latest version before editing again.';
			} else error = 'Could not confirm the change. Retry to recover the result.';
		} finally {
			if (current === generation) working = false;
		}
	}

	async function refresh() {
		pendingUpdate = null;
		showForm = false;
		editing = null;
		error = '';
		await loadPage(meetingId, 1, false);
	}
</script>

<section
	class="card-compact card mx-4 mt-3 border border-base-300 bg-base-100 shadow-sm"
	aria-label="Meeting actions"
>
	<div class="card-body gap-3">
		<div class="flex flex-wrap items-center justify-between gap-2">
			<div>
				<h2 class="card-title text-base">Actions</h2>
				<p class="text-xs text-base-content/60">
					Actions remain with this meeting. Assignment does not grant meeting access.
				</p>
			</div>
			<div class="flex gap-2">
				<button
					class="btn btn-ghost btn-xs"
					disabled={loading || working}
					onclick={() => void refresh()}>Refresh actions</button
				><button
					class="btn btn-outline btn-xs"
					disabled={working || !!pendingUpdate}
					onclick={() => void openForm(null)}>Create action</button
				>
			</div>
		</div>
		{#if error}<p class="text-sm text-error" role="alert">{error}</p>{/if}
		{#if pendingUpdate && !working}<button
				class="btn self-start btn-sm"
				onclick={() => void update()}>Retry change</button
			>{/if}
		{#if showForm}
			<form
				class="flex flex-col gap-3 rounded border border-base-300 p-3"
				onsubmit={(event) => {
					event.preventDefault();
					void save();
				}}
			>
				<fieldset
					class="flex flex-col gap-3"
					disabled={working || !!pendingUpdate || (!editing && !!pendingCreate)}
				>
					<label class="form-control"
						><span class="label-text">Action title</span><input
							class="input-bordered input w-full input-sm"
							bind:value={title}
							required
							maxlength="255"
						/></label
					>
					<label class="form-control"
						><span class="label-text">Action description</span><textarea
							class="textarea-bordered textarea w-full"
							bind:value={description}
							maxlength="16384"></textarea></label
					>
					<label class="form-control"
						><span class="label-text">Responsible person</span><select
							class="select-bordered select w-full select-sm"
							value={assigneeId}
							onchange={(event) => {
								assigneeId = event.currentTarget.value;
							}}
						>
							<option value="">Unassigned</option>
							{#if assigneeId && !people.some((person) => person.id === assigneeId)}<option
									value={assigneeId}
									>{editing?.assignee?.display_name ?? 'Current assignee unavailable'}</option
								>{/if}
							{#each people as person (person.id)}<option value={person.id}
									>{person.display_name}</option
								>{/each}
						</select></label
					>
					{#if !peoplePage || people.length < peopleTotal}<button
							type="button"
							class="btn self-start btn-ghost btn-xs"
							disabled={loadingOptions}
							onclick={() => void loadOptions('people')}>Load more people</button
						>{/if}
					<label class="form-control"
						><span class="label-text">Due date</span><input
							type="date"
							class="input-bordered input w-full input-sm"
							bind:value={dueDate}
						/></label
					>
					<p class="text-xs text-base-content/60">
						Due dates are calendar days, without a time or timezone conversion.
					</p>
					<label class="form-control"
						><span class="label-text">Related Decision</span><select
							class="select-bordered select w-full select-sm"
							value={decisionId}
							onchange={(event) => {
								decisionId = event.currentTarget.value;
							}}
						>
							<option value=""
								>{editing && !editing.related_decision_available
									? 'Keep existing reference, if any'
									: 'None'}</option
							>
							{#if decisionId && !decisions.some((link) => link.decision?.id === decisionId)}<option
									value={decisionId}
									>{editing?.related_decision?.title ?? 'Current Decision'}</option
								>{/if}
							{#each decisions as link (link.id)}{#if link.available && link.decision}<option
										value={link.decision.id}>{link.decision.title}</option
									>{/if}{/each}
						</select></label
					>
					{#if !decisionsPage || decisions.length < decisionsTotal}<button
							type="button"
							class="btn self-start btn-ghost btn-xs"
							disabled={loadingOptions}
							onclick={() => void loadOptions('decisions')}>Load more Decisions</button
						>{/if}
				</fieldset>
				<div class="flex gap-2">
					<button class="btn btn-primary btn-sm" disabled={working || !title.trim()}
						>{editing
							? 'Save action'
							: pendingCreate
								? 'Retry creation'
								: 'Save new action'}</button
					>
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						disabled={working}
						onclick={() => {
							showForm = false;
							editing = null;
						}}>Close</button
					>
				</div>
			</form>
		{/if}
		{#if loading && !actions.length}<p class="text-sm text-base-content/60">
				Loading actions…
			</p>{:else if actions.length}
			<ul class="divide-y divide-base-300">
				{#each actions as action (action.id)}
					<li class="flex flex-wrap items-start justify-between gap-3 py-3">
						<div class="min-w-0 flex-1">
							<h3 class="text-sm font-medium">{action.title}</h3>
							{#if action.description}<p class="mt-1 text-sm whitespace-pre-wrap">
									{action.description}
								</p>{/if}
							<p class="mt-1 text-xs text-base-content/60">
								{action.state === 'completed' ? 'Completed' : 'Open'} · Responsible: {action
									.assignee?.display_name ??
									(action.assignee_id ? 'Unavailable' : 'Unassigned')}{#if action.due_date}
									· Due <time datetime={action.due_date}>{action.due_date}</time>{/if}
							</p>
							{#if action.related_decision_available && action.related_decision}<a
									class="link text-xs"
									href={action.related_decision.open_url}>{action.related_decision.title}</a
								>{/if}
						</div>
						<div class="flex gap-1">
							<button
								class="btn btn-ghost btn-xs"
								disabled={working || !!pendingUpdate}
								aria-label={`Edit ${action.title}`}
								onclick={() => void openForm(action)}>Edit</button
							><button
								class="btn btn-outline btn-xs"
								disabled={working || !!pendingUpdate}
								aria-label={`${action.state === 'open' ? 'Complete' : 'Reopen'} ${action.title}`}
								onclick={() => void toggle(action)}
								>{action.state === 'open' ? 'Complete' : 'Reopen'}</button
							>
						</div>
					</li>
				{/each}
			</ul>
		{:else if !loading}<p class="text-sm text-base-content/60">No actions yet.</p>{/if}
		{#if total > actions.length}<button
				class="btn self-start btn-ghost btn-xs"
				disabled={loading || working}
				onclick={() => void loadPage(meetingId, page + 1, true)}>Load more actions</button
			>{/if}
	</div>
</section>
