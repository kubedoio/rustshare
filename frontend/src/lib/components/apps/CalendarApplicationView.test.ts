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
		timezone: null,
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

	it('creates an internal event from the editor', async () => {
		render(CalendarApplicationView, { module: testModule });
		await fireEvent.click(await screen.findByRole('button', { name: /New event/ }));

		await fireEvent.input(screen.getByLabelText('Event title'), { target: { value: 'Dentist' } });
		await fireEvent.input(screen.getByLabelText('Date'), {
			target: { value: dayFromToday(1) }
		});
		await fireEvent.submit(screen.getByRole('dialog', { name: 'Create event' }));

		await waitFor(() => expect(mocks.createEvent).toHaveBeenCalledTimes(1));
		expect(mocks.createEvent).toHaveBeenCalledWith(expect.objectContaining({ title: 'Dentist' }));
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
		await fireEvent.submit(screen.getByRole('dialog', { name: 'Edit event' }));

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
});
