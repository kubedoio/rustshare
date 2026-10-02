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

	// Create requires a concrete IANA zone. The browser normally reports one,
	// but a non-string/empty value falls back to UTC rather than sending null
	// (which the backend rejects as invalid JSON).
	const BROWSER_TIMEZONE = (() => {
		const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
		return typeof tz === 'string' && tz.length > 0 ? tz : 'UTC';
	})();

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

	const eventsQuery = createQuery<CalendarEvent[]>(buildEventQueryOptions());

	$effect(() => {
		eventsQuery.setOptions(buildEventQueryOptions());
	});

	function buildEventQueryOptions() {
		return {
			queryKey: eventQueryKey as unknown as string[],
			queryFn: () =>
				calendarApi.listEvents({
					from: fromIso,
					to: toIso,
					sourceIds: activeSourceId ? [activeSourceId] : undefined
				})
		};
	}

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
			timezone: string;
		}) => {
			if (input.id) {
				// Empty string clears description/location; the backend leaves
				// them unchanged for null/absent (F7 convention).
				return calendarApi.updateEvent(input.id, {
					title: input.title,
					location: input.location,
					description: input.description,
					starts_at: input.starts_at,
					ends_at: input.ends_at,
					timezone: input.timezone
				});
			}
			return calendarApi.createEvent({
				title: input.title,
				location: input.location || null,
				description: input.description || null,
				starts_at: input.starts_at,
				ends_at: input.ends_at,
				timezone: input.timezone
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

	// Calendar-date arithmetic on YYYY-MM-DD keys. UTC avoids the DST shifts
	// that local Date construction would introduce.
	function dayKeyToUtc(key: string): number {
		const [year, month, day] = key.split('-').map(Number);
		return Date.UTC(year, month - 1, day);
	}

	function daysBetween(startKey: string, endKey: string): number {
		return Math.round((dayKeyToUtc(endKey) - dayKeyToUtc(startKey)) / DAY_MS);
	}

	function addDaysToKey(key: string, days: number): string {
		return new Date(dayKeyToUtc(key) + days * DAY_MS).toISOString().slice(0, 10);
	}

	// All-day events carry their wall-clock date in original_date; the
	// starts_at/ends_at instants are UTC midnights that can shift to the
	// previous local day for negative-offset users. Compare calendar-date
	// fields directly, and treat the all-day ends_at as an exclusive DTEND.
	// Expanded all-day occurrences carry instance_start too, so derive the
	// displayed occurrence day from it (falling back to original_date) and
	// shift the exclusive end by the master's whole-day duration.
	function eventsOn(events: CalendarEvent[], date: Date): CalendarEvent[] {
		// Local day boundaries, not dayStart + 24h: DST transition days are
		// 23h/25h long, so a fixed DAY_MS window misbuckets their events.
		const dayStart = startOfDay(date).getTime();
		const dayEnd = startOfDay(addDays(date, 1)).getTime();
		const dayKey = toLocalInputDate(date);
		return events
			.filter((event) => {
				if (event.all_day) {
					const startDay = (event.instance_start ?? event.original_date ?? event.starts_at).slice(
						0,
						10
					);
					const masterStartDay = (event.original_date ?? event.starts_at).slice(0, 10);
					const durationDays = Math.max(1, daysBetween(masterStartDay, event.ends_at.slice(0, 10)));
					const endDay = addDaysToKey(startDay, durationDays);
					return startDay <= dayKey && dayKey < endDay;
				}
				const start = occurrenceStart(event).getTime();
				const end = occurrenceEnd(event).getTime();
				return start < dayEnd && end > dayStart;
			})
			.sort((a, b) => occurrenceStart(a).getTime() - occurrenceStart(b).getTime());
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

	// Range responses carry the occurrence instant only in `instance_start`;
	// `starts_at`/`ends_at` remain the recurring master's values. All-day
	// occurrences carry instance_start as well, so the all-day branch of
	// eventsOn derives its date from it (falling back to original_date).
	function occurrenceStart(event: CalendarEvent): Date {
		return new Date(event.instance_start ?? event.starts_at);
	}

	function occurrenceEnd(event: CalendarEvent): Date {
		if (!event.instance_start) return new Date(event.ends_at);
		const duration = new Date(event.ends_at).getTime() - new Date(event.starts_at).getTime();
		return new Date(new Date(event.instance_start).getTime() + duration);
	}

	/** An expanded instance of a recurring series: same id as the master. */
	function isOccurrence(event: CalendarEvent): boolean {
		return event.instance_start != null;
	}

	function eventDetailDateLabel(event: CalendarEvent): string {
		if (event.all_day) {
			const key = (event.instance_start ?? event.original_date ?? event.starts_at).slice(0, 10);
			return new Date(`${key}T00:00:00`).toLocaleDateString();
		}
		return occurrenceStart(event).toLocaleDateString();
	}

	function eventTimeLabel(event: CalendarEvent): string {
		if (event.all_day) return 'All day';
		return `${formatTime(occurrenceStart(event).toISOString())} – ${formatTime(
			occurrenceEnd(event).toISOString()
		)}`;
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

	// `wholeSeries` seeds the form from the stored (master) times when editing
	// an expanded occurrence's series; the occurrence instant must never be
	// PATCHed back as the master's starts_at.
	function openEditor(event: CalendarEvent, wholeSeries = false) {
		const start = wholeSeries ? new Date(event.starts_at) : occurrenceStart(event);
		const end = wholeSeries ? new Date(event.ends_at) : occurrenceEnd(event);
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
			ends_at: end.toISOString(),
			// Create requires a concrete IANA zone; reuse the master's zone when
			// editing, otherwise fall back to the browser's zone (or UTC).
			timezone: editingEvent?.timezone ?? BROWSER_TIMEZONE
		});
	}

	// Expanded occurrences carry the master's id, so an occurrence's Delete
	// removes the whole series — the confirm text must say so rather than
	// imply a single-occurrence delete.
	async function handleDelete(event: CalendarEvent) {
		const message = isOccurrence(event)
			? `Delete the entire recurring series "${event.title}"? All occurrences will be removed.`
			: `Delete "${event.title}"?`;
		if (!confirm(message)) return;
		await deleteMutation.mutateAsync(event.id);
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		if (editorOpen) editorOpen = false;
		else if (selectedEvent) selectedEvent = null;
	}

	const WEEKDAY_LABELS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
</script>

<svelte:window onkeydown={handleKeydown} />

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
											{#if !event.all_day}{formatTime(occurrenceStart(event).toISOString())}{/if}
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
											{#if !event.all_day}{formatTime(occurrenceStart(event).toISOString())}{/if}
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
												{eventTimeLabel(event)}
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
				{eventDetailDateLabel(selectedEvent)}
				{#if selectedEvent.all_day}
					· All day
				{:else}
					· {formatTime(occurrenceStart(selectedEvent).toISOString())} – {formatTime(
						occurrenceEnd(selectedEvent).toISOString()
					)}
				{/if}
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
			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				{#if !selectedEvent.read_only}
					{#if isOccurrence(selectedEvent)}
						<button
							type="button"
							class="btn btn-outline btn-error btn-sm"
							onclick={() => handleDelete(selectedEvent!)}
						>
							Delete entire series
						</button>
						{#if selectedEvent.all_day}
							<p class="w-full text-right text-xs text-base-content/60">
								All-day events cannot be edited here yet. Delete and recreate it to change its
								dates.
							</p>
						{:else}
							<button
								type="button"
								class="btn btn-primary btn-sm"
								onclick={() => openEditor(selectedEvent!, true)}
							>
								<Pencil size={13} /> Edit series
							</button>
							<p class="w-full text-right text-xs text-base-content/60">
								This is one occurrence of a recurring series. Editing or deleting here affects the
								entire series.
							</p>
						{/if}
					{:else}
						<button
							type="button"
							class="btn btn-outline btn-error btn-sm"
							onclick={() => handleDelete(selectedEvent!)}
						>
							Delete
						</button>
						{#if selectedEvent.all_day}
							<p class="w-full text-right text-xs text-base-content/60">
								All-day events cannot be edited here yet. Delete and recreate it to change its
								dates.
							</p>
						{:else}
							<button
								type="button"
								class="btn btn-primary btn-sm"
								onclick={() => openEditor(selectedEvent!)}
							>
								<Pencil size={13} /> Edit event
							</button>
						{/if}
					{/if}
				{/if}
			</div>
		</div>
	</div>
{/if}

<!-- Create / edit modal -->
{#if editorOpen}
	<div
		class="fixed inset-0 z-20 flex items-center justify-center bg-black/40 p-4"
		role="dialog"
		aria-modal="true"
		tabindex="-1"
		aria-label={editingEvent ? 'Edit event' : 'Create event'}
		onclick={(event) => {
			if (event.target === event.currentTarget) editorOpen = false;
		}}
		onkeydown={(event) => {
			if (event.key === 'Escape') editorOpen = false;
		}}
	>
		<form
			class="flex w-full max-w-md flex-col gap-3 rounded-xl border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] p-4"
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
			{#if editingEvent?.rrule}
				<p class="text-xs text-warning" role="note">
					This is a recurring event; saving changes updates the entire series.
				</p>
			{/if}
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
