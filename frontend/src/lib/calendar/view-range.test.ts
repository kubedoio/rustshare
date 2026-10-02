import { describe, expect, it } from 'vitest';
import { VIEW_OPTIONS, startOfWeekMonday, windowRange, shiftWindow } from './view-range';

const at = (s: string) => new Date(s);

describe('windowRange', () => {
	it('day view covers exactly the local day', () => {
		const { from, to } = windowRange('day', at('2026-10-14T15:00:00'));
		expect(from.getHours()).toBe(0);
		expect(to.getTime() - from.getTime()).toBe(24 * 3600 * 1000);
	});

	it('work week runs Monday 00:00 to Saturday 00:00 (5 days)', () => {
		const { from, to } = windowRange('week', at('2026-10-14T15:00:00')); // Wednesday
		expect(from.getDay()).toBe(1); // Monday
		expect(from.getDate()).toBe(12);
		expect(to.getDate()).toBe(17); // exclusive Saturday
		expect(to.getTime() - from.getTime()).toBe(5 * 24 * 3600 * 1000);
	});

	it('month view stays Sunday-anchored over 42 days', () => {
		const { from, to } = windowRange('month', at('2026-10-14T15:00:00'));
		expect(from.getDay()).toBe(0);
		expect(Math.round((to.getTime() - from.getTime()) / 86400000)).toBe(42);
	});

	it('agenda covers 30 days', () => {
		const { from, to } = windowRange('agenda', at('2026-10-14T15:00:00'));
		expect(Math.round((to.getTime() - from.getTime()) / 86400000)).toBe(30);
	});
});

describe('shiftWindow', () => {
	it('steps day/week/month/agenda by one period', () => {
		expect(shiftWindow('day', at('2026-10-14T10:00:00'), 1).getDate()).toBe(15);
		expect(shiftWindow('week', at('2026-10-14T10:00:00'), 1).getDate()).toBe(21);
		expect(shiftWindow('month', at('2026-10-14T10:00:00'), -1).getMonth()).toBe(8);
		expect(shiftWindow('agenda', at('2026-10-14T10:00:00'), 1).getDate()).toBe(13); // +30d
	});
});

describe('startOfWeekMonday', () => {
	it('is idempotent and lands on Monday for every weekday', () => {
		for (const d of [
			'2026-10-12',
			'2026-10-13',
			'2026-10-14',
			'2026-10-15',
			'2026-10-16',
			'2026-10-17',
			'2026-10-18'
		]) {
			const monday = startOfWeekMonday(at(`${d}T12:00:00`));
			expect(monday.getDay()).toBe(1);
			expect(startOfWeekMonday(monday).getTime()).toBe(monday.getTime());
		}
	});
});

describe('VIEW_OPTIONS', () => {
	it('exposes day, work week, month and agenda with labels', () => {
		expect(VIEW_OPTIONS.map((v) => v.id)).toEqual(['day', 'week', 'month', 'agenda']);
		expect(VIEW_OPTIONS.map((v) => v.label)).toEqual(['Day', 'Work week', 'Month', 'Agenda']);
	});
});
