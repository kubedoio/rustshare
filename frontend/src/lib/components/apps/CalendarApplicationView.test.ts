import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import { toastStore } from '$lib/stores/toast';
import type { ApplicationDefinition } from '$lib/applications/registry';
import type { CalendarEvent, CalendarSource } from '$lib/api/calendar';
import CalendarApplicationView from './CalendarApplicationView.svelte';

const mocks = vi.hoisted(() => ({
	listEvents: vi.fn(),
	createEvent: vi.fn(),
	updateEvent: vi.fn(),
	deleteEvent: vi.fn(),
	listSources: vi.fn(),
	createIcsSource: vi.fn(),
	updateSource: vi.fn(),
	deleteSource: vi.fn(),
	uploadIcs: vi.fn(),
	listImportJobs: vi.fn(),
	getImportJob: vi.fn()
}));

vi.mock('$lib/api/calendar', () => ({
	calendarApi: mocks
}));

const testModule = {
	key: 'calendar',
	name: 'Calendar',
	description: 'Calendar workspace',
	icon: 'calendar-days',
	enabled: true,
	dashboard: { enabled: false },
	page: { enabled: true, route: '/apps/calendar', renderer: 'calendar', layout: 'list-grid' },
	aiIndexing: { enabled: false },
	audit: { enabled: true }
} as unknown as ApplicationDefinition;

const internalSource: CalendarSource = {
	id: 'src-internal',
	kind: 'internal',
	display_name: 'My calendar',
	external_account: null,
	external_calendar_id: null,
	is_enabled: true,
	status: 'healthy',
	last_synced_at: null,
	last_error: null,
	created_at: '2026-10-01T00:00:00Z'
};

const googleSource: CalendarSource = {
	id: 'src-google',
	kind: 'google',
	display_name: 'Work Google',
	external_account: 'user@example.com',
	external_calendar_id: 'primary',
	is_enabled: true,
	status: 'healthy',
	last_synced_at: '2026-10-01T00:00:00Z',
	last_error: null,
	created_at: '2026-10-01T00:00:00Z'
};

/** Local YYYY-MM-DD for `days` from today. */
function dayFromToday(days: number): string {
	const date = new Date();
	date.setDate(date.getDate() + days);
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${date.getFullYear()}-${month}-${day}`;
}

function eventAt(date: string, overrides: Partial<CalendarEvent> = {}): CalendarEvent {
	return {
		id: `evt-${date}-${overrides.title ?? 'x'}`,
		source_id: internalSource.id,
		source_kind: 'internal',
		title: 'Sprint review',
		description: null,
		location: null,
		starts_at: `${date}T14:00:00`,
		ends_at: `${date}T15:00:00`,
		all_day: false,
		original_date: null,
		timezone: 'UTC',
		rrule: null,
		recurrence_id: null,
		instance_start: null,
		status: 'confirmed',
		read_only: false,
		created_at: '2026-10-01T00:00:00Z',
		updated_at: '2026-10-01T00:00:00Z',
		...overrides
	};
}

describe('CalendarApplicationView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		localStorage.clear();
		sessionStorage.clear();
		queryClient.clear();
		toastStore.clear();
		mocks.listSources.mockResolvedValue([internalSource, googleSource]);
		mocks.listEvents.mockResolvedValue([]);
		mocks.createEvent.mockResolvedValue(eventAt(dayFromToday(1)));
		mocks.updateEvent.mockResolvedValue(eventAt(dayFromToday(1)));
		mocks.deleteEvent.mockResolvedValue(undefined);
	});

	it('renders events in the visible month', async () => {
		const inWindow = dayFromToday(3);
		const outOfWindow = dayFromToday(90);
		mocks.listEvents.mockResolvedValue([
			eventAt(inWindow),
			eventAt(outOfWindow, { title: 'Far away' })
		]);
		render(CalendarApplicationView, { module: testModule });

		expect(await screen.findByText(/Sprint review/)).toBeTruthy();
		expect(screen.queryByText(/Far away/)).toBeNull();
	});

	it('requests the full visible month window', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');

		const first = new Date();
		first.setDate(1);
		const windowStart = new Date(first.getFullYear(), first.getMonth(), 1 - first.getDay());
		const windowEnd = new Date(windowStart);
		windowEnd.setDate(windowStart.getDate() + 42);

		expect(mocks.listEvents).toHaveBeenCalledWith(
			expect.objectContaining({
				from: windowStart.toISOString(),
				to: windowEnd.toISOString()
			})
		);
	});

	it('shows read-only events without an edit affordance and with source attribution', async () => {
		mocks.listEvents.mockResolvedValue([
			eventAt(dayFromToday(1), {
				id: 'evt-g-1',
				source_id: googleSource.id,
				source_kind: 'google',
				title: 'Provider sync',
				read_only: true
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Provider sync/));
		expect(await screen.findByText('from Google — user@example.com')).toBeTruthy();
		expect(screen.queryByRole('button', { name: /Edit event/ })).toBeNull();
	});

	it('offers editing for internal events', async () => {
		mocks.listEvents.mockResolvedValue([eventAt(dayFromToday(1))]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Sprint review/));
		expect(await screen.findByRole('button', { name: /Edit event/ })).toBeTruthy();
	});

	it('refetches with the source filter when a source chip is toggled', async () => {
		mocks.listEvents.mockResolvedValue([]);
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');

		mocks.listEvents.mockClear();
		await fireEvent.click(screen.getByRole('button', { name: /Work Google/ }));

		await waitFor(() =>
			expect(mocks.listEvents).toHaveBeenCalledWith(
				expect.objectContaining({ sourceIds: ['src-google'] })
			)
		);
	});

	it('clears the source filter on a second toggle', async () => {
		mocks.listEvents.mockResolvedValue([]);
		render(CalendarApplicationView, { module: testModule });
		const chip = (await screen.findByRole('button', { name: /Work Google/ })) as HTMLButtonElement;

		await fireEvent.click(chip);
		await waitFor(() =>
			expect(mocks.listEvents).toHaveBeenCalledWith(
				expect.objectContaining({ sourceIds: ['src-google'] })
			)
		);

		await fireEvent.click(await screen.findByRole('button', { name: /Work Google/ }));
		await waitFor(() =>
			expect(
				(screen.getByRole('button', { name: /Work Google/ }) as HTMLButtonElement).getAttribute(
					'aria-pressed'
				)
			).toBe('false')
		);
	});

	it('creates an internal event from the editor with a concrete IANA timezone', async () => {
		render(CalendarApplicationView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /New event/ }));

		await fireEvent.input(screen.getByLabelText('Event title'), { target: { value: 'Dentist' } });
		await fireEvent.input(screen.getByLabelText('Date'), {
			target: { value: dayFromToday(1) }
		});
		await fireEvent.submit(screen.getByRole('form', { name: 'Create event' }));

		await waitFor(() => expect(mocks.createEvent).toHaveBeenCalledTimes(1));
		const payload = mocks.createEvent.mock.calls[0][0] as { timezone: unknown };
		expect(payload).toMatchObject({ title: 'Dentist' });
		// The create request requires a non-null IANA zone.
		expect(typeof payload.timezone).toBe('string');
		expect(payload.timezone).toBe(Intl.DateTimeFormat().resolvedOptions().timeZone);
	});

	it('creates an event from a day cell with that cell date and a concrete timezone', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');
		await goToMonth(2026, 9);

		const cellLabel = new Date(2026, 9, 14, 12).toLocaleDateString();
		await fireEvent.click(screen.getByRole('button', { name: `Create event on ${cellLabel}` }));

		await fireEvent.input(screen.getByLabelText('Event title'), {
			target: { value: 'Board meeting' }
		});
		await fireEvent.submit(screen.getByRole('form', { name: 'Create event' }));

		await waitFor(() => expect(mocks.createEvent).toHaveBeenCalledTimes(1));
		const payload = mocks.createEvent.mock.calls[0][0] as {
			title: string;
			starts_at: string;
			timezone: unknown;
		};
		expect(payload.title).toBe('Board meeting');
		// The day cell's date, at the editor's default 09:00 start.
		expect(payload.starts_at).toBe(new Date(2026, 9, 14, 9, 0, 0).toISOString());
		// Same concrete zone the sibling create test asserts.
		expect(payload.timezone).toBe(Intl.DateTimeFormat().resolvedOptions().timeZone);
	});

	it('falls back to UTC when the browser reports no timezone', async () => {
		const spy = vi
			.spyOn(Intl, 'DateTimeFormat')
			.mockReturnValue({ resolvedOptions: () => ({}) } as unknown as Intl.DateTimeFormat);
		try {
			render(CalendarApplicationView, { module: testModule });
			await fireEvent.click(await screen.findByRole('button', { name: /New event/i }));
			await fireEvent.input(screen.getByLabelText('Event title'), {
				target: { value: 'TZ fallback' }
			});
			await fireEvent.submit(screen.getByRole('form', { name: 'Create event' }));

			await waitFor(() => expect(mocks.createEvent).toHaveBeenCalledTimes(1));
			expect(mocks.createEvent.mock.calls[0][0].timezone).toBe('UTC');
		} finally {
			spy.mockRestore();
		}
	});

	it('buckets all-day events by original_date and treats ends_at as exclusive', async () => {
		const day = dayFromToday(3);
		const nextDay = dayFromToday(4);
		// starts_at instant falls on the PREVIOUS UTC day (mimics a
		// negative-offset user seeing a UTC-midnight all-day start); the
		// original_date wall-clock date must win.
		mocks.listEvents.mockResolvedValue([
			eventAt(day, {
				id: 'evt-allday',
				title: 'All hands',
				all_day: true,
				original_date: day,
				starts_at: `${day}T00:00:00+14:00`,
				ends_at: `${nextDay}T00:00:00+14:00`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		const dayLabel = new Date(`${day}T12:00:00`).toLocaleDateString();
		const cell = (
			await screen.findByRole('button', { name: `Create event on ${dayLabel}` })
		).closest('.min-h-24')!;
		expect(cell.textContent).toContain('All hands');

		// Exclusive DTEND: the following day must not show the event.
		const nextDayLabel = new Date(`${nextDay}T12:00:00`).toLocaleDateString();
		const nextCell = screen
			.getByRole('button', { name: `Create event on ${nextDayLabel}` })
			.closest('.min-h-24')!;
		expect(nextCell.textContent).not.toContain('All hands');
	});

	it('renders multi-day events on every covered day', async () => {
		const start = dayFromToday(3);
		const middle = dayFromToday(4);
		const end = dayFromToday(5);
		mocks.listEvents.mockResolvedValue([
			eventAt(start, {
				id: 'evt-multiday',
				title: 'Roadtrip',
				starts_at: `${start}T22:00:00`,
				ends_at: `${end}T02:00:00`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		// Covers the start day, the full middle day, and the end day.
		expect((await screen.findAllByText(/Roadtrip/)).length).toBe(3);
		for (const day of [start, middle, end]) {
			const label = new Date(`${day}T12:00:00`).toLocaleDateString();
			const cell = screen
				.getByRole('button', { name: `Create event on ${label}` })
				.closest('.min-h-24')!;
			expect(cell.textContent).toContain('Roadtrip');
		}
	});

	it('preserves the existing timezone when editing an event', async () => {
		mocks.listEvents.mockResolvedValue([
			eventAt(dayFromToday(1), { id: 'evt-tz', timezone: 'Europe/Berlin' })
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Sprint review/));
		await fireEvent.click(await screen.findByRole('button', { name: /Edit event/ }));
		await fireEvent.submit(screen.getByRole('form', { name: 'Edit event' }));

		await waitFor(() => expect(mocks.updateEvent).toHaveBeenCalledTimes(1));
		expect(mocks.updateEvent).toHaveBeenCalledWith(
			'evt-tz',
			expect.objectContaining({ timezone: 'Europe/Berlin' })
		);
	});

	it('points the empty state at the calendar settings page', async () => {
		render(CalendarApplicationView, { module: testModule });

		expect(await screen.findByText('No events in this period')).toBeTruthy();
		const link = screen.getByRole('link', { name: 'Open Calendar settings' });
		expect(link.getAttribute('href')).toBe('/settings/apps/calendar');
	});

	it('buckets recurring occurrences by instance_start across different days', async () => {
		// A weekly master: starts_at/ends_at stay the master's values and each
		// expanded occurrence carries the master's id plus its own instant in
		// instance_start.
		const masterDay = dayFromToday(2);
		const occurrenceDays = [dayFromToday(2), dayFromToday(9), dayFromToday(16)];
		mocks.listEvents.mockResolvedValue(
			occurrenceDays.map((day) =>
				eventAt(masterDay, {
					id: 'evt-weekly',
					title: 'Weekly sync',
					rrule: 'FREQ=WEEKLY;COUNT=3',
					starts_at: `${masterDay}T14:00:00`,
					ends_at: `${masterDay}T15:00:00`,
					instance_start: `${day}T14:00:00`
				})
			)
		);
		render(CalendarApplicationView, { module: testModule });

		const chips = await screen.findAllByText(/Weekly sync/);
		expect(chips).toHaveLength(3);
		// Each chip must render in its own day cell, not stacked on the
		// master's day: the cell's "Create event on …" label is unique per day.
		const cellLabels = chips.map((chip) =>
			chip
				.closest('.min-h-24')
				?.querySelector('button[aria-label^="Create event on"]')
				?.getAttribute('aria-label')
		);
		expect(new Set(cellLabels).size).toBe(3);
	});

	// Navigate the month cursor to a given month regardless of the current date.
	async function goToMonth(year: number, month: number) {
		const now = new Date();
		const diff = (year - now.getFullYear()) * 12 + (month - now.getMonth());
		const name = diff >= 0 ? 'Next period' : 'Previous period';
		for (let i = 0; i < Math.abs(diff); i++) {
			await fireEvent.click(await screen.findByRole('button', { name }));
		}
	}

	function cellForDay(label: string): HTMLElement {
		return screen.getByRole('button', { name: `Create event on ${label}` }).closest('.min-h-24')!;
	}

	it('renders an all-day recurring occurrence on its instance date, not the master date', async () => {
		// Weekly all-day master starting 2026-10-01. The expanded occurrence on
		// 2026-10-08 reuses the master's id/original_date but carries its own
		// instance_start, which must win when picking the displayed day.
		mocks.listEvents.mockResolvedValue([
			eventAt('2026-10-01', {
				id: 'evt-allday-series',
				title: 'Company holiday',
				all_day: true,
				original_date: '2026-10-01',
				starts_at: '2026-10-01T00:00:00Z',
				ends_at: '2026-10-02T00:00:00Z',
				rrule: 'FREQ=WEEKLY;COUNT=4',
				instance_start: '2026-10-08T00:00:00Z'
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		const occurrenceLabel = new Date('2026-10-08T12:00:00').toLocaleDateString();
		const occurrenceCell = await screen.findByRole('button', {
			name: `Create event on ${occurrenceLabel}`
		});
		expect(occurrenceCell.closest('.min-h-24')!.textContent).toContain('Company holiday');

		const masterLabel = new Date('2026-10-01T12:00:00').toLocaleDateString();
		expect(cellForDay(masterLabel).textContent).not.toContain('Company holiday');
	});

	it('offers a series edit for an occurrence and warns the whole series changes', async () => {
		const masterDay = dayFromToday(2);
		const occurrenceDay = dayFromToday(9);
		mocks.listEvents.mockResolvedValue([
			eventAt(masterDay, {
				id: 'evt-weekly',
				title: 'Weekly sync',
				rrule: 'FREQ=WEEKLY;COUNT=3',
				starts_at: `${masterDay}T14:00:00`,
				ends_at: `${masterDay}T15:00:00`,
				instance_start: `${occurrenceDay}T14:00:00`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Weekly sync/));

		// The occurrence must not pretend to be a single-event edit.
		expect(screen.queryByRole('button', { name: /Edit event/ })).toBeNull();
		expect(await screen.findByText(/one occurrence of a recurring series/)).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: /Edit series/ }));
		expect(await screen.findByText(/updates the entire series/)).toBeTruthy();
	});

	it('saves a series edit with the master id and master starts_at, not the occurrence instant', async () => {
		const masterDay = dayFromToday(2);
		const occurrenceDay = dayFromToday(9);
		mocks.listEvents.mockResolvedValue([
			eventAt(masterDay, {
				id: 'evt-weekly',
				title: 'Weekly sync',
				rrule: 'FREQ=WEEKLY;COUNT=3',
				starts_at: `${masterDay}T14:00:00`,
				ends_at: `${masterDay}T15:00:00`,
				instance_start: `${occurrenceDay}T14:00:00`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Weekly sync/));
		await fireEvent.click(screen.getByRole('button', { name: /Edit series/ }));
		await fireEvent.submit(screen.getByRole('form', { name: 'Edit event' }));

		await waitFor(() => expect(mocks.updateEvent).toHaveBeenCalledTimes(1));
		const [id, payload] = mocks.updateEvent.mock.calls[0] as [string, { starts_at: string }];
		expect(id).toBe('evt-weekly');
		// Master start (local day), never the occurrence instant's day.
		expect(new Date(payload.starts_at).toLocaleDateString()).toBe(
			new Date(`${masterDay}T14:00:00`).toLocaleDateString()
		);
		expect(new Date(payload.starts_at).toLocaleDateString()).not.toBe(
			new Date(`${occurrenceDay}T14:00:00`).toLocaleDateString()
		);
	});

	it('confirms that deleting an occurrence deletes the entire series', async () => {
		const confirmMock = vi.fn(() => false);
		vi.stubGlobal('confirm', confirmMock);
		try {
			const masterDay = dayFromToday(2);
			mocks.listEvents.mockResolvedValue([
				eventAt(masterDay, {
					id: 'evt-weekly',
					title: 'Weekly sync',
					rrule: 'FREQ=WEEKLY;COUNT=3',
					starts_at: `${masterDay}T14:00:00`,
					ends_at: `${masterDay}T15:00:00`,
					instance_start: `${dayFromToday(9)}T14:00:00`
				})
			]);
			render(CalendarApplicationView, { module: testModule });

			await fireEvent.click(await screen.findByText(/Weekly sync/));
			await fireEvent.click(await screen.findByRole('button', { name: 'Delete entire series' }));

			expect(confirmMock).toHaveBeenCalledWith(expect.stringMatching(/entire recurring series/));
			// Declined confirm must not issue the delete.
			expect(mocks.deleteEvent).not.toHaveBeenCalled();
		} finally {
			vi.unstubAllGlobals();
		}
	});

	it('shows all-day as "All day" and does not offer editing for all-day events', async () => {
		const day = dayFromToday(3);
		mocks.listEvents.mockResolvedValue([
			eventAt(day, {
				id: 'evt-allday',
				title: 'All hands',
				all_day: true,
				original_date: day,
				starts_at: `${day}T00:00:00Z`,
				ends_at: `${dayFromToday(4)}T00:00:00Z`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/All hands/));

		// Detail popover must not render a midnight clock range.
		expect(await screen.findByText(/All day/)).toBeTruthy();
		expect(await screen.findByText(/All-day events cannot be edited/)).toBeTruthy();
		expect(screen.queryByRole('button', { name: /Edit event/ })).toBeNull();
	});

	it('shows the all-day explanation and no series edit for an all-day recurring occurrence', async () => {
		const masterDay = dayFromToday(2);
		const occurrenceDay = dayFromToday(9);
		mocks.listEvents.mockResolvedValue([
			eventAt(masterDay, {
				id: 'evt-allday-series',
				title: 'Company holiday',
				all_day: true,
				original_date: masterDay,
				starts_at: `${masterDay}T00:00:00Z`,
				ends_at: `${dayFromToday(3)}T00:00:00Z`,
				rrule: 'FREQ=WEEKLY;COUNT=3',
				instance_start: `${occurrenceDay}T00:00:00Z`
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Company holiday/));

		// The occurrence's all-day span cannot round-trip through the editor, so
		// it must not offer "Edit series" — only the all-day explanation.
		expect(await screen.findByText(/All-day events cannot be edited/)).toBeTruthy();
		expect(screen.queryByRole('button', { name: /Edit series/ })).toBeNull();
	});

	it('clears description and location by sending empty strings on update', async () => {
		mocks.listEvents.mockResolvedValue([
			eventAt(dayFromToday(1), {
				id: 'evt-clear',
				title: 'Dentist',
				location: 'Room 1',
				description: 'Bring forms'
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Dentist/));
		await fireEvent.click(await screen.findByRole('button', { name: /Edit event/ }));
		await fireEvent.input(screen.getByLabelText('Location'), { target: { value: '' } });
		await fireEvent.input(screen.getByLabelText('Description'), { target: { value: '' } });
		await fireEvent.submit(screen.getByRole('form', { name: 'Edit event' }));

		await waitFor(() => expect(mocks.updateEvent).toHaveBeenCalledTimes(1));
		expect(mocks.updateEvent).toHaveBeenCalledWith(
			'evt-clear',
			expect.objectContaining({ location: '', description: '' })
		);
	});

	it('closes the detail popover on Escape', async () => {
		mocks.listEvents.mockResolvedValue([eventAt(dayFromToday(1))]);
		render(CalendarApplicationView, { module: testModule });

		await fireEvent.click(await screen.findByText(/Sprint review/));
		expect(await screen.findByRole('dialog', { name: 'Event details' })).toBeTruthy();

		await fireEvent.keyDown(window, { key: 'Escape' });

		await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Event details' })).toBeNull());
	});

	it('buckets a fall-back DST day event by local day boundaries', async () => {
		const originalTz = process.env.TZ;
		process.env.TZ = 'America/New_York';
		try {
			// 2026-11-01 is 25h long; 23:30 -> 00:30 spans the extra hour.
			const start = new Date(2026, 10, 1, 23, 30);
			const end = new Date(2026, 10, 2, 0, 30);
			mocks.listEvents.mockResolvedValue([
				eventAt('2026-11-01', {
					id: 'evt-dst-fall',
					title: 'Late night',
					starts_at: start.toISOString(),
					ends_at: end.toISOString()
				})
			]);
			render(CalendarApplicationView, { module: testModule });
			await goToMonth(2026, 10);

			const label = new Date(2026, 10, 1, 12).toLocaleDateString();
			expect(cellForDay(label).textContent).toContain('Late night');
		} finally {
			process.env.TZ = originalTz;
		}
	});

	it('does not leak a next-day early event into a spring-forward DST day', async () => {
		const originalTz = process.env.TZ;
		process.env.TZ = 'America/New_York';
		try {
			// 2026-03-08 is 23h long; an event on 03-09 00:30 must not appear
			// on 03-08 (a fixed 24h window would wrongly include it).
			const start = new Date(2026, 2, 9, 0, 30);
			const end = new Date(2026, 2, 9, 1, 30);
			mocks.listEvents.mockResolvedValue([
				eventAt('2026-03-09', {
					id: 'evt-dst-spring',
					title: 'Early bird',
					starts_at: start.toISOString(),
					ends_at: end.toISOString()
				})
			]);
			render(CalendarApplicationView, { module: testModule });
			await goToMonth(2026, 2);

			const mar8 = new Date(2026, 2, 8, 12).toLocaleDateString();
			const mar9 = new Date(2026, 2, 9, 12).toLocaleDateString();
			expect(cellForDay(mar9).textContent).toContain('Early bird');
			expect(cellForDay(mar8).textContent).not.toContain('Early bird');
		} finally {
			process.env.TZ = originalTz;
		}
	});

	function lastEventQuery(): { from: string; to: string } {
		return mocks.listEvents.mock.calls.at(-1)?.[0] as { from: string; to: string };
	}

	function localInputDate(date: Date): string {
		const month = String(date.getMonth() + 1).padStart(2, '0');
		const day = String(date.getDate()).padStart(2, '0');
		return `${date.getFullYear()}-${month}-${day}`;
	}

	/** Monday 00:00 (local) of the week containing `date`. */
	function mondayOf(date: Date): Date {
		const day = new Date(date.getFullYear(), date.getMonth(), date.getDate());
		return new Date(day.getFullYear(), day.getMonth(), day.getDate() - ((day.getDay() + 6) % 7));
	}

	/** The Sunday that closes the current (Monday-first) work week. */
	function sundayOfCurrentWeek(): Date {
		const monday = mondayOf(new Date());
		return new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 6);
	}

	/** Wire-shaped event: UTC instants like the range API returns. */
	function utcEventAt(startLocal: Date, overrides: Partial<CalendarEvent> = {}): CalendarEvent {
		const end = new Date(startLocal.getTime() + 60 * 60 * 1000);
		return eventAt(localInputDate(startLocal), {
			starts_at: startLocal.toISOString(),
			ends_at: end.toISOString(),
			...overrides
		});
	}

	it('day view requests exactly one local day and shows the day heading', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');
		await fireEvent.click(screen.getByRole('button', { name: 'Day' }));

		const now = new Date();
		const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
		const nextMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
		await waitFor(() =>
			expect(lastEventQuery()).toEqual({
				from: midnight.toISOString(),
				to: nextMidnight.toISOString()
			})
		);

		const heading = midnight.toLocaleDateString(undefined, {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			year: 'numeric'
		});
		expect(screen.getByText(heading)).toBeTruthy();
	});

	it('work week view requests Monday 00:00 through Saturday 00:00 and hides weekends', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');
		await fireEvent.click(screen.getByRole('button', { name: 'Work week' }));

		const monday = mondayOf(new Date());
		const saturday = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 5);
		await waitFor(() =>
			expect(lastEventQuery()).toEqual({
				from: monday.toISOString(),
				to: saturday.toISOString()
			})
		);
		expect(new Date(lastEventQuery().from).getDay()).toBe(1);
		expect(
			new Date(lastEventQuery().to).getTime() - new Date(lastEventQuery().from).getTime()
		).toBe(5 * 24 * 3600 * 1000);

		for (const label of ['Mon', 'Tue', 'Wed', 'Thu', 'Fri']) {
			expect(screen.getByText(label)).toBeTruthy();
		}
		expect(screen.queryByText('Sat')).toBeNull();
		expect(screen.queryByText('Sun')).toBeNull();
	});

	it('work week excludes a Sunday event', async () => {
		const sunday = sundayOfCurrentWeek();
		mocks.listEvents.mockResolvedValue([
			utcEventAt(new Date(sunday.getFullYear(), sunday.getMonth(), sunday.getDate(), 12), {
				id: 'evt-sunday',
				title: 'Sunday brunch'
			})
		]);
		render(CalendarApplicationView, { module: testModule });

		// Month view (Sunday-first) still renders it.
		expect(await screen.findByText(/Sunday brunch/)).toBeTruthy();

		await fireEvent.click(screen.getByRole('button', { name: 'Work week' }));
		await waitFor(() => expect(screen.queryByText(/Sunday brunch/)).toBeNull());
	});

	it('navigation steps by one day in day view and one week in work week', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');

		await fireEvent.click(screen.getByRole('button', { name: 'Day' }));
		await waitFor(() => expect(mocks.listEvents).toHaveBeenCalled());
		const dayBefore = new Date(lastEventQuery().from);
		const dayExpected = new Date(dayBefore);
		dayExpected.setDate(dayExpected.getDate() + 1);

		mocks.listEvents.mockClear();
		await fireEvent.click(screen.getByRole('button', { name: 'Next period' }));
		await waitFor(() =>
			expect(new Date(lastEventQuery().from).getTime()).toBe(dayExpected.getTime())
		);

		await fireEvent.click(screen.getByRole('button', { name: 'Work week' }));
		await waitFor(() => expect(new Date(lastEventQuery().from).getDay()).toBe(1));
		const weekBefore = new Date(lastEventQuery().from);
		const weekExpected = new Date(weekBefore);
		weekExpected.setDate(weekExpected.getDate() + 7);

		mocks.listEvents.mockClear();
		await fireEvent.click(screen.getByRole('button', { name: 'Next period' }));
		await waitFor(() => {
			const from = new Date(lastEventQuery().from);
			expect(from.getTime()).toBe(weekExpected.getTime());
			expect(from.getDay()).toBe(1);
		});
	});

	it('shows a create affordance in work-week cells and in day hour rows', async () => {
		render(CalendarApplicationView, { module: testModule });
		await screen.findByLabelText('Filter by source');

		await fireEvent.click(screen.getByRole('button', { name: 'Work week' }));
		const monday = mondayOf(new Date());
		await fireEvent.click(
			await screen.findByRole('button', {
				name: `Create event on ${monday.toLocaleDateString()}`
			})
		);
		expect((screen.getByLabelText('Date') as HTMLInputElement).value).toBe(localInputDate(monday));
		await fireEvent.click(screen.getByRole('button', { name: 'Close editor' }));

		await fireEvent.click(screen.getByRole('button', { name: 'Day' }));
		const today = new Date();
		const todayLabel = new Date(
			today.getFullYear(),
			today.getMonth(),
			today.getDate()
		).toLocaleDateString();
		await fireEvent.click(
			await screen.findByRole('button', { name: `Create event on ${todayLabel} at 14:00` })
		);
		expect((screen.getByLabelText('Date') as HTMLInputElement).value).toBe(
			localInputDate(new Date(today.getFullYear(), today.getMonth(), today.getDate()))
		);
		expect((screen.getByLabelText('Start') as HTMLInputElement).value).toBe('14:00');
	});

	it('places a timed UTC event on its wall-clock row in day view', async () => {
		const originalTz = process.env.TZ;
		process.env.TZ = 'UTC';
		try {
			const day = dayFromToday(0);
			mocks.listEvents.mockResolvedValue([
				utcEventAt(new Date(`${day}T10:00:00Z`), { id: 'evt-timed', title: 'Standup' })
			]);
			render(CalendarApplicationView, { module: testModule });
			await screen.findByLabelText('Filter by source');
			await fireEvent.click(screen.getByRole('button', { name: 'Day' }));

			const chip = await screen.findByRole('button', { name: /Standup/ });
			// 10:00 -> 600 minutes into a 1440-minute grid; one hour tall.
			expect(parseFloat(chip.style.top)).toBeCloseTo((600 / 1440) * 100, 1);
			expect(parseFloat(chip.style.height)).toBeCloseTo((60 / 1440) * 100, 1);
		} finally {
			process.env.TZ = originalTz;
		}
	});

	it('places a spring-forward event on its wall-clock hour despite the skipped hour', async () => {
		const originalTz = process.env.TZ;
		process.env.TZ = 'America/New_York';
		vi.useFakeTimers({ toFake: ['Date'] });
		try {
			const springForward = new Date(2026, 2, 8, 12, 0, 0); // 2026-03-08 skips 02:00 -> 03:00.
			vi.setSystemTime(springForward);
			mocks.listEvents.mockResolvedValue([
				utcEventAt(new Date(2026, 2, 8, 3, 0, 0), {
					id: 'evt-dst-day',
					title: 'After the jump'
				})
			]);
			render(CalendarApplicationView, { module: testModule });
			await screen.findByLabelText('Filter by source');
			await fireEvent.click(screen.getByRole('button', { name: 'Day' }));

			const chip = await screen.findByRole('button', { name: /After the jump/ });
			// 03:00 wall clock -> the 180-minute row, not the 120-minute elapsed row.
			expect(parseFloat(chip.style.top)).toBeCloseTo((180 / 1440) * 100, 1);
		} finally {
			vi.useRealTimers();
			process.env.TZ = originalTz;
		}
	});

	it('clamps a multi-day event inside the day grid', async () => {
		const originalTz = process.env.TZ;
		process.env.TZ = 'UTC';
		try {
			const day = dayFromToday(0);
			const next = dayFromToday(1);
			mocks.listEvents.mockResolvedValue([
				eventAt(day, {
					id: 'evt-overnight',
					title: 'Overnight deploy',
					starts_at: `${day}T22:00:00Z`,
					ends_at: `${next}T02:00:00Z`
				})
			]);
			render(CalendarApplicationView, { module: testModule });
			await screen.findByLabelText('Filter by source');
			await fireEvent.click(screen.getByRole('button', { name: 'Day' }));

			const chip = await screen.findByRole('button', { name: /Overnight deploy/ });
			const top = parseFloat(chip.style.top);
			const height = parseFloat(chip.style.height);
			expect(top).toBeCloseTo((22 / 24) * 100, 1);
			expect(height).toBeGreaterThan(0);
			expect(top + height).toBeLessThanOrEqual(100.001);
		} finally {
			process.env.TZ = originalTz;
		}
	});
});
