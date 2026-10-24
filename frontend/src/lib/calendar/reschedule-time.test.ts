import { describe, expect, it, vi } from 'vitest';
import { eventDateForCalendarDate, localDateTime, rescheduleTimedEvent } from './reschedule-time';

const event = (starts_at: string, ends_at: string, all_day = false) => ({
	starts_at,
	ends_at,
	all_day
});

describe('calendar event-local rescheduling', () => {
	it('keeps the Berlin wall time over the spring DST transition', () => {
		expect(
			rescheduleTimedEvent(
				event('2026-03-28T09:00:00Z', '2026-03-28T10:30:00Z'),
				'Europe/Berlin',
				'2026-03-29'
			)
		).toEqual({ starts_at: '2026-03-29T08:00:00.000Z', ends_at: '2026-03-29T09:30:00.000Z' });
	});

	it('keeps the Berlin wall time over the autumn DST transition', () => {
		expect(
			rescheduleTimedEvent(
				event('2026-10-24T08:00:00Z', '2026-10-24T09:30:00Z'),
				'Europe/Berlin',
				'2026-10-25'
			)
		).toEqual({ starts_at: '2026-10-25T09:00:00.000Z', ends_at: '2026-10-25T10:30:00.000Z' });
	});

	it('rejects a nonexistent spring wall time and picks the earlier fall overlap', () => {
		expect(() =>
			rescheduleTimedEvent(
				event('2026-03-28T01:30:00Z', '2026-03-28T02:00:00Z'),
				'Europe/Berlin',
				'2026-03-29',
				'02:30'
			)
		).toThrow(/does not exist/);
		expect(
			rescheduleTimedEvent(
				event('2026-10-24T00:30:00Z', '2026-10-24T01:00:00Z'),
				'Europe/Berlin',
				'2026-10-25',
				'02:30'
			).starts_at
		).toBe('2026-10-25T00:30:00.000Z');
	});

	it('preserves elapsed duration across midnight and year boundaries', () => {
		expect(
			rescheduleTimedEvent(
				event('2026-06-10T21:30:00Z', '2026-06-11T01:00:00Z'),
				'UTC',
				'2027-01-01'
			)
		).toEqual({ starts_at: '2027-01-01T21:30:00.000Z', ends_at: '2027-01-02T01:00:00.000Z' });
	});

	it('uses the event zone even when the browser zone differs', () => {
		vi.stubEnv('TZ', 'America/Los_Angeles');
		try {
			const instant = '2026-10-24T08:00:00Z';
			expect(Intl.DateTimeFormat().resolvedOptions().timeZone).toBe('America/Los_Angeles');
			expect(new Date(instant).getHours()).toBe(1);
			expect(localDateTime(instant, 'Europe/Berlin')).toEqual({
				date: '2026-10-24',
				time: '10:00'
			});
		} finally {
			vi.unstubAllEnvs();
		}
	});

	it('moves a browser-displayed day while preserving the event-zone wall date offset', () => {
		vi.stubEnv('TZ', 'America/Los_Angeles');
		try {
			const starts_at = '2026-10-24T22:30:00Z';
			const eventDate = eventDateForCalendarDate(starts_at, 'Europe/Berlin', '2026-10-25');
			expect(localDateTime(starts_at, 'Europe/Berlin').date).toBe('2026-10-25');
			expect(localDateTime(starts_at, 'America/Los_Angeles').date).toBe('2026-10-24');
			expect(eventDate).toBe('2026-10-26');
			const moved = rescheduleTimedEvent(
				event(starts_at, '2026-10-24T23:30:00Z'),
				'Europe/Berlin',
				eventDate
			);
			expect(moved.starts_at).toBe('2026-10-25T23:30:00.000Z');
			expect(localDateTime(moved.starts_at, 'America/Los_Angeles').date).toBe('2026-10-25');
		} finally {
			vi.unstubAllEnvs();
		}
	});

	it('rejects all-day events and non-positive durations', () => {
		expect(() =>
			rescheduleTimedEvent(
				event('2026-10-24T00:00:00Z', '2026-10-25T00:00:00Z', true),
				'UTC',
				'2026-10-26'
			)
		).toThrow(/All-day/);
		expect(() =>
			rescheduleTimedEvent(
				event('2026-10-24T10:00:00Z', '2026-10-24T10:00:00Z'),
				'UTC',
				'2026-10-25'
			)
		).toThrow(/positive duration/);
	});
});
