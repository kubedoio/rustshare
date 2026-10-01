<script lang="ts">
	import { createMutation, createQuery } from '$lib/query-compat';
	import {
		calendarApi,
		type CalendarEvent,
		type CalendarSource,
		type CalendarSourceKind
	} from '$lib/api/calendar';
	import { queryClient } from '$lib/query-client';
	import ApplicationPageShell from '$lib/components/layout/ApplicationPageShell.svelte';
	import ApplicationPageSkeleton from '$lib/components/common/ApplicationPageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { CalendarDays, ChevronLeft, ChevronRight, Pencil, Plus, X } from 'lucide-svelte';
	import type { ApplicationDefinition } from '$lib/applications/registry';

	let { module }: { module: ApplicationDefinition } = $props();

	type CalendarView = 'month' | 'week' | 'agenda';

	const DAY_MS = 24 * 60 * 60 * 1000;

	let view = $state<CalendarView>('month');
	let cursor = $state<Date>(startOfDay(new Date()));
	let activeSourceId = $state<string | null>(null);
	let selectedEvent = $state<CalendarEvent | null>(null);
	let editorOpen = $state(false);
	let editingEvent = $state<CalendarEvent | null>(null);
	let formTitle = $state('');
	let formDate = $state('');
	let formStartTime = $state('09:00');
	let formEndTime = $state('10:00');
	let formLocation = $state('');
	let formDescription = $state('');

	function startOfDay(date: Date): Date {
		return new Date(date.getFullYear(), date.getMonth(), date.getDate());
	}

	function addDays(date: Date, days: number): Date {
		return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
	}

	function toLocalInputDate(date: Date): string {
		const month = String(date.getMonth() + 1).padStart(2, '0');
		const day = String(date.getDate()).padStart(2, '0');
		return `${date.getFullYear()}-${month}-${day}`;
	}

	const windowRange = $derived.by((): { from: Date; to: Date } => {
		if (view === 'month') {
			const firstOfMonth = new Date(cursor.getFullYear(), cursor.getMonth(), 1);
			const offset = firstOfMonth.getDay();
			const from = addDays(firstOfMonth, -offset);
			return { from, to: addDays(from, 42) };
		}
		if (view === 'week') {
			const from = addDays(cursor, -cursor.getDay());
			return { from, to: addDays(from, 7) };
		}
		return { from: cursor, to: addDays(cursor, 30) };
	});

	const fromIso = $derived(windowRange.from.toISOString());
	const toIso = $derived(windowRange.to.toISOString());
	const eventQueryKey = $derived(
		activeSourceId
			? (['calendar-events', fromIso, toIso, activeSourceId] as const)
			: (['calendar-events', fromIso, toIso] as const)
	);

	const eventsQuery = createQuery<CalendarEvent[]>({
		queryKey: eventQueryKey as unknown as string[],
		queryFn: () =>
			calendarApi.listEvents({
				from: fromIso,
				to: toIso,
				sourceIds: activeSourceId ? [activeSourceId] : undefined
			})
	});

	$effect(() => {
		eventsQuery.setOptions({
			queryKey: eventQueryKey as unknown as string[],
			queryFn: () =>
				calendarApi.listEvents({
					from: fromIso,
					to: toIso,
					sourceIds: activeSourceId ? [activeSourceId] : undefined
				})
		});
	});

	const sourcesQuery = createQuery<CalendarSource[]>({
		queryKey: ['calendar-sources'],
		queryFn: () => calendarApi.listSources()
	});

	const saveMutation = createMutation({
		mutationFn: async (input: {
			id: string | null;
			title: string;
			location: string;
			description: string;
			starts_at: string;
			ends_at: string;
		}) => {
			if (input.id) {
				return calendarApi.updateEvent(input.id, {
					title: input.title,
					location: input.location || null,
					description: input.description || null,
					starts_at: input.starts_at,
					ends_at: input.ends_at
				});
			}
			return calendarApi.createEvent({
				title: input.title,
				location: input.location || null,
				description: input.description || null,
				starts_at: input.starts_at,
				ends_at: input.ends_at
			});
		},
		onSuccess: async (_data, variables) => {
			editorOpen = false;
			editingEvent = null;
			toastStore.show(variables.id ? 'Event updated' : 'Event created', 'success');
			await queryClient.invalidateQueries({ queryKey: ['calendar-events'] });
		},
		onError: (error) => {
			toastStore.show(
				`Failed to save event: ${error instanceof Error ? error.message : 'unknown error'}`,
				'error'
			);
		}
	});

	const deleteMutation = createMutation({
		mutationFn: (eventId: string) => calendarApi.deleteEvent(eventId),
		onSuccess: async () => {
			selectedEvent = null;
			editorOpen = false;
			editingEvent = null;
			toastStore.show('Event deleted', 'success');
			await queryClient.invalidateQueries({ queryKey: ['calendar-events'] });
		},
		onError: (error) => {
			toastStore.show(
				`Failed to delete event: ${error instanceof Error ? error.message : 'unknown error'}`,
				'error'
			);
		}
	});

	const KIND_COLORS: Record<CalendarSourceKind, string> = {
		internal: 'bg-brand-500',
		ical_import: 'bg-info',
		google: 'bg-success',
		outlook: 'bg-warning'
	};

	const KIND_LABELS: Record<CalendarSourceKind, string> = {
		internal: 'Internal',
		ical_import: 'iCal import',
		google: 'Google',
		outlook: 'Outlook'
	};

	const externalSources = $derived(($sourcesQuery.data ?? []).filter((s) => s.kind !== 'internal'));

	function kindColor(kind: CalendarSourceKind): string {
		return KIND_COLORS[kind] ?? 'bg-base-content/40';
	}

	function eventColor(event: CalendarEvent): string {
		return kindColor(event.source_kind);
	}

	const monthCells = $derived.by(
		(): Array<{ date: Date; inMonth: boolean; events: CalendarEvent[] }> => {
			const events = $eventsQuery.data ?? [];
			const cells = [];
			for (let i = 0; i < 42; i++) {
				const date = addDays(windowRange.from, i);
				cells.push({
					date,
					inMonth: date.getMonth() === cursor.getMonth(),
					events: eventsOn(events, date)
				});
			}
			return cells;
		}
	);

	const weekCells = $derived.by((): Array<{ date: Date; events: CalendarEvent[] }> => {
		const events = $eventsQuery.data ?? [];
		return Array.from({ length: 7 }, (_, i) => {
			const date = addDays(windowRange.from, i);
			return { date, events: eventsOn(events, date) };
		});
	});

	const agendaGroups = $derived.by((): Array<{ date: Date; events: CalendarEvent[] }> => {
		const events = $eventsQuery.data ?? [];
		const groups: Array<{ date: Date; events: CalendarEvent[] }> = [];
		for (let i = 0; i < 30; i++) {
			const date = addDays(windowRange.from, i);
			const dayEvents = eventsOn(events, date);
			if (dayEvents.length > 0) groups.push({ date, events: dayEvents });
		}
		return groups;
	});

	const windowEvents = $derived($eventsQuery.data ?? []);

	const heading = $derived(
		view === 'month'
			? cursor.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
			: `${windowRange.from.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })} – ${addDays(windowRange.to, -1).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })}`
	);

	function eventsOn(events: CalendarEvent[], date: Date): CalendarEvent[] {
		return events
			.filter((event) => {
				const start = new Date(event.starts_at);
				return (
					start.getFullYear() === date.getFullYear() &&
					start.getMonth() === date.getMonth() &&
					start.getDate() === date.getDate()
				);
			})
			.sort((a, b) => a.starts_at.localeCompare(b.starts_at));
	}

	function shiftWindow(direction: 1 | -1) {
		if (view === 'month') {
			cursor = new Date(cursor.getFullYear(), cursor.getMonth() + direction, 1);
		} else {
			cursor = addDays(cursor, direction * (view === 'week' ? 7 : 30));
		}
	}

	function toggleSource(sourceId: string) {
		activeSourceId = activeSourceId === sourceId ? null : sourceId;
	}

	function formatTime(iso: string): string {
		return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
	}

	function sourceAttribution(event: CalendarEvent): string {
		const source = ($sourcesQuery.data ?? []).find((s) => s.id === event.source_id);
		const kindLabel = KIND_LABELS[event.source_kind] ?? event.source_kind;
		const account = source?.external_account;
		return account ? `from ${kindLabel} — ${account}` : `from ${kindLabel}`;
	}

	function openCreate(date?: Date) {
		editingEvent = null;
		formTitle = '';
		formDate = toLocalInputDate(date ?? cursor);
		formStartTime = '09:00';
		formEndTime = '10:00';
		formLocation = '';
		formDescription = '';
		editorOpen = true;
	}

	function openEditor(event: CalendarEvent) {
		const start = new Date(event.starts_at);
		const end = new Date(event.ends_at);
		editingEvent = event;
		formTitle = event.title;
		formDate = toLocalInputDate(start);
		formStartTime = start.toTimeString().slice(0, 5);
		formEndTime = end.toTimeString().slice(0, 5);
		formLocation = event.location ?? '';
		formDescription = event.description ?? '';
		selectedEvent = null;
		editorOpen = true;
	}

	function openDetail(event: CalendarEvent) {
		selectedEvent = event;
	}

	async function handleSave() {
		const start = new Date(`${formDate}T${formStartTime}:00`);
		const end = new Date(`${formDate}T${formEndTime}:00`);
		if (!formTitle.trim() || Number.isNaN(start.getTime()) || end <= start) {
			toastStore.show('Enter a title and an end time after the start time.', 'error');
			return;
		}
		await saveMutation.mutateAsync({
			id: editingEvent?.id ?? null,
			title: formTitle.trim(),
			location: formLocation.trim(),
			description: formDescription.trim(),
			starts_at: start.toISOString(),
			ends_at: end.toISOString()
		});
	}

	async function handleDelete(event: CalendarEvent) {
		if (!confirm(`Delete "${event.title}"?`)) return;
		await deleteMutation.mutateAsync(event.id);
	}

	const WEEKDAY_LABELS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
</script>

<ApplicationPageShell title={module.displayName} subtitle={module.description}>
	{#if $eventsQuery.isLoading}
		<ApplicationPageSkeleton />
	{:else if $eventsQuery.isError}
		<ErrorState
			title="Calendar could not be loaded."
			message={$eventsQuery.error instanceof Error ? $eventsQuery.error.message : undefined}
			onRetry={() => eventsQuery.refetch()}
			retryLabel="Retry"
		/>
	{:else}
		<div class="flex flex-col gap-4">
			<!-- Toolbar: navigation, view switcher, source filters -->
			<div
				class="flex flex-col gap-3 rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-3 lg:flex-row lg:items-center lg:justify-between"
			>
				<div class="flex items-center gap-2">
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						aria-label="Previous period"
						onclick={() => shiftWindow(-1)}
					>
						<ChevronLeft size={16} />
					</button>
					<span class="min-w-40 text-center text-sm font-semibold text-base-content">{heading}</span
					>
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						aria-label="Next period"
						onclick={() => shiftWindow(1)}
					>
						<ChevronRight size={16} />
					</button>
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						onclick={() => (cursor = startOfDay(new Date()))}
					>
						Today
					</button>
				</div>

				<div class="flex flex-wrap items-center gap-2">
					{#each ['month', 'week', 'agenda'] as CalendarView[] as option}
						<button
							type="button"
							class="btn btn-sm {view === option ? 'btn-primary' : 'btn-outline'}"
							aria-pressed={view === option}
							onclick={() => (view = option)}
						>
							{option[0].toUpperCase() + option.slice(1)}
						</button>
					{/each}
				</div>
			</div>

			<!-- Source filter chips -->
			{#if externalSources.length > 0}
				<div class="flex flex-wrap items-center gap-2" aria-label="Filter by source">
					{#each externalSources as source}
						<button
							type="button"
							class="btn gap-1.5 btn-xs {activeSourceId === source.id
								? 'btn-primary'
								: 'btn-outline'}"
							aria-pressed={activeSourceId === source.id}
							onclick={() => toggleSource(source.id)}
						>
							<span class="h-2 w-2 rounded-full {kindColor(source.kind)}"></span>
							{source.display_name}
						</button>
					{/each}
					{#if activeSourceId}
						<button
							type="button"
							class="btn btn-ghost btn-xs"
							onclick={() => (activeSourceId = null)}
						>
							<X size={12} /> Clear filter
						</button>
					{/if}
				</div>
			{/if}

			<!-- Calendar body -->
			{#if view === 'month'}
				<div
					class="overflow-hidden rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]"
				>
					<div class="grid grid-cols-7 border-b border-[var(--rs-border)]">
						{#each WEEKDAY_LABELS as label}
							<div class="px-2 py-1.5 text-center text-2xs font-semibold text-base-content/50">
								{label}
							</div>
						{/each}
					</div>
					<div class="grid grid-cols-7">
						{#each monthCells as cell}
							<div
								class="min-h-24 border-r border-b border-[var(--rs-border)] p-1 {cell.inMonth
									? 'bg-[var(--rs-surface-raised)]'
									: 'bg-base-200/40'} {(cell.date.getDay() + 1) % 7 === 0 ? 'border-r-0' : ''}"
							>
								<div class="flex items-center justify-between">
									<span
										class="flex h-6 w-6 items-center justify-center rounded-full text-2xs {cell.inMonth
											? 'text-base-content/80'
											: 'text-base-content/35'} {cell.date.getTime() ===
										startOfDay(new Date()).getTime()
											? 'bg-brand-500 font-bold text-white'
											: ''}"
									>
										{cell.date.getDate()}
									</span>
									<button
										type="button"
										class="btn btn-ghost px-1 opacity-0 transition-opacity btn-xs [div:hover>&]:opacity-100"
										aria-label="Create event on {cell.date.toLocaleDateString()}"
										onclick={() => openCreate(cell.date)}
									>
										<Plus size={12} />
									</button>
								</div>
								<div class="mt-0.5 flex flex-col gap-0.5">
									{#each cell.events as event}
										<button
											type="button"
											class="truncate rounded px-1 py-0.5 text-left text-2xs text-white {eventColor(
												event
											)}"
											onclick={() => openDetail(event)}
										>
											{formatTime(event.starts_at)}
											{event.title}
										</button>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				</div>
			{:else if view === 'week'}
				<div
					class="overflow-hidden rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)]"
				>
					<div class="grid grid-cols-7 border-b border-[var(--rs-border)]">
						{#each weekCells as cell}
							<div class="px-2 py-1.5 text-center">
								<div class="text-2xs font-semibold text-base-content/50">
									{WEEKDAY_LABELS[cell.date.getDay()]}
								</div>
								<div class="text-sm font-semibold text-base-content">{cell.date.getDate()}</div>
							</div>
						{/each}
					</div>
					<div class="grid grid-cols-7">
						{#each weekCells as cell}
							<div class="min-h-32 border-r border-[var(--rs-border)] p-1 last:border-r-0">
								<div class="flex flex-col gap-0.5">
									{#each cell.events as event}
										<button
											type="button"
											class="truncate rounded px-1 py-0.5 text-left text-2xs text-white {eventColor(
												event
											)}"
											onclick={() => openDetail(event)}
										>
											{formatTime(event.starts_at)}
											{event.title}
										</button>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				</div>
			{:else}
				<div class="flex flex-col gap-2">
					{#if agendaGroups.length === 0}
						<div
							class="flex flex-col items-center rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-6 py-12 text-center"
						>
							<CalendarDays size={28} class="text-base-content/20" />
							<p class="mt-3 text-sm font-medium text-base-content">
								No events in the next 30 days
							</p>
							<a href="/settings/apps/calendar" class="btn mt-4 btn-primary btn-sm">
								Import or connect a calendar
							</a>
						</div>
					{:else}
						{#each agendaGroups as group}
							<div
								class="rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-3"
							>
								<h3 class="text-xs font-semibold text-base-content/60">
									{group.date.toLocaleDateString(undefined, {
										weekday: 'long',
										month: 'long',
										day: 'numeric'
									})}
								</h3>
								<div class="mt-2 flex flex-col gap-1">
									{#each group.events as event}
										<button
											type="button"
											class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-base-200"
											onclick={() => openDetail(event)}
										>
											<span class="h-2 w-2 shrink-0 rounded-full {eventColor(event)}"></span>
											<span class="w-24 shrink-0 text-2xs text-base-content/60">
												{formatTime(event.starts_at)} – {formatTime(event.ends_at)}
											</span>
											<span class="truncate text-sm text-base-content">{event.title}</span>
										</button>
									{/each}
								</div>
							</div>
						{/each}
					{/if}
				</div>
			{/if}

			<!-- Empty window state -->
			{#if view !== 'agenda' && windowEvents.length === 0}
				<div
					class="flex flex-col items-center rounded-xl border border-dashed border-[var(--rs-border)] bg-[var(--rs-surface-raised)] px-6 py-10 text-center"
				>
					<CalendarDays size={28} class="text-base-content/20" />
					<p class="mt-3 text-sm font-medium text-base-content">No events in this period</p>
					<p class="mt-1 text-xs text-base-content/60">
						Create one with + on a day, or import and connect calendars from the settings.
					</p>
					<a href="/settings/apps/calendar" class="btn mt-4 btn-outline btn-sm">
						Open Calendar settings
					</a>
				</div>
			{/if}

			<!-- Floating create button -->
			<button
				type="button"
				class="btn fixed right-6 bottom-6 z-10 gap-1.5 shadow-lg btn-primary btn-sm"
				onclick={() => openCreate()}
			>
				<Plus size={14} /> New event
			</button>
		</div>
	{/if}
</ApplicationPageShell>

<!-- Detail popover -->
{#if selectedEvent}
	<div
		class="fixed inset-0 z-20 flex items-center justify-center bg-black/40 p-4"
		role="presentation"
		onclick={(event) => {
			if (event.target === event.currentTarget) selectedEvent = null;
		}}
	>
		<div
			class="w-full max-w-md rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-4"
			role="dialog"
			aria-label="Event details"
		>
			<div class="flex items-start justify-between gap-3">
				<h3 class="text-base font-semibold text-base-content">{selectedEvent.title}</h3>
				<button
					type="button"
					class="btn btn-ghost btn-xs"
					aria-label="Close event details"
					onclick={() => (selectedEvent = null)}
				>
					<X size={14} />
				</button>
			</div>
			<p class="mt-2 text-sm text-base-content/70">
				{new Date(selectedEvent.starts_at).toLocaleString()} – {new Date(
					selectedEvent.ends_at
				).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}
			</p>
			{#if selectedEvent.location}
				<p class="mt-1 text-sm text-base-content/70">{selectedEvent.location}</p>
			{/if}
			{#if selectedEvent.description}
				<p class="mt-2 text-sm whitespace-pre-wrap text-base-content/80">
					{selectedEvent.description}
				</p>
			{/if}
			{#if selectedEvent.read_only}
				<p class="mt-3 text-xs text-base-content/50">{sourceAttribution(selectedEvent)}</p>
			{/if}
			<div class="mt-4 flex justify-end gap-2">
				{#if !selectedEvent.read_only}
					<button
						type="button"
						class="btn btn-outline btn-error btn-sm"
						onclick={() => handleDelete(selectedEvent!)}
					>
						Delete
					</button>
					<button
						type="button"
						class="btn btn-primary btn-sm"
						onclick={() => openEditor(selectedEvent!)}
					>
						<Pencil size={13} /> Edit event
					</button>
				{/if}
			</div>
		</div>
	</div>
{/if}

<!-- Create / edit modal -->
{#if editorOpen}
	<div
		class="fixed inset-0 z-20 flex items-center justify-center bg-black/40 p-4"
		role="presentation"
		onclick={(event) => {
			if (event.target === event.currentTarget) editorOpen = false;
		}}
	>
		<form
			class="flex w-full max-w-md flex-col gap-3 rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-4"
			role="dialog"
			aria-label={editingEvent ? 'Edit event' : 'Create event'}
			onsubmit={(event) => {
				event.preventDefault();
				handleSave();
			}}
		>
			<div class="flex items-center justify-between">
				<h3 class="text-base font-semibold text-base-content">
					{editingEvent ? 'Edit event' : 'New event'}
				</h3>
				<button
					type="button"
					class="btn btn-ghost btn-xs"
					aria-label="Close editor"
					onclick={() => (editorOpen = false)}
				>
					<X size={14} />
				</button>
			</div>
			<input
				class="input-bordered input input-sm"
				placeholder="Event title"
				aria-label="Event title"
				bind:value={formTitle}
				required
			/>
			<div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
				<div class="form-control">
					<label class="label py-1 text-xs font-semibold" for="cal-event-date">Date</label>
					<input
						id="cal-event-date"
						type="date"
						class="input-bordered input input-sm"
						bind:value={formDate}
						required
					/>
				</div>
				<div class="form-control">
					<label class="label py-1 text-xs font-semibold" for="cal-event-start">Start</label>
					<input
						id="cal-event-start"
						type="time"
						class="input-bordered input input-sm"
						bind:value={formStartTime}
						required
					/>
				</div>
				<div class="form-control">
					<label class="label py-1 text-xs font-semibold" for="cal-event-end">End</label>
					<input
						id="cal-event-end"
						type="time"
						class="input-bordered input input-sm"
						bind:value={formEndTime}
						required
					/>
				</div>
			</div>
			<input
				class="input-bordered input input-sm"
				placeholder="Location (optional)"
				aria-label="Location"
				bind:value={formLocation}
			/>
			<textarea
				class="textarea-bordered textarea textarea-sm"
				placeholder="Description (optional)"
				aria-label="Description"
				rows="3"
				bind:value={formDescription}></textarea>
			<div class="flex justify-end gap-2">
				<button type="button" class="btn btn-outline btn-sm" onclick={() => (editorOpen = false)}>
					Cancel
				</button>
				<button type="submit" class="btn btn-primary btn-sm" disabled={$saveMutation.isPending}>
					{#if $saveMutation.isPending}<span class="loading loading-xs loading-spinner"></span>{/if}
					{editingEvent ? 'Save changes' : 'Create event'}
				</button>
			</div>
		</form>
	</div>
{/if}
