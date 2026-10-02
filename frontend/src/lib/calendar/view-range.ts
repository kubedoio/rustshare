export type CalendarView = 'day' | 'week' | 'month' | 'agenda';

export const VIEW_OPTIONS: { id: CalendarView; label: string }[] = [
	{ id: 'day', label: 'Day' },
	{ id: 'week', label: 'Work week' },
	{ id: 'month', label: 'Month' },
	{ id: 'agenda', label: 'Agenda' }
];

export function startOfDay(date: Date): Date {
	return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function addDays(date: Date, days: number): Date {
	return new Date(
		date.getFullYear(),
		date.getMonth(),
		date.getDate() + days,
		date.getHours(),
		date.getMinutes(),
		date.getSeconds()
	);
}

export function startOfWeekMonday(date: Date): Date {
	const day = startOfDay(date);
	const offset = (day.getDay() + 6) % 7; // Monday = 0
	return addDays(day, -offset);
}

export function windowRange(view: CalendarView, cursor: Date): { from: Date; to: Date } {
	const day = startOfDay(cursor);
	switch (view) {
		case 'day':
			return { from: day, to: addDays(day, 1) };
		case 'week': {
			const monday = startOfWeekMonday(day);
			return { from: monday, to: addDays(monday, 5) };
		}
		case 'month': {
			const first = new Date(day.getFullYear(), day.getMonth(), 1);
			const from = addDays(first, -first.getDay());
			return { from, to: addDays(from, 42) };
		}
		case 'agenda':
			return { from: day, to: addDays(day, 30) };
	}
}

export function shiftWindow(view: CalendarView, cursor: Date, direction: 1 | -1): Date {
	switch (view) {
		case 'day':
			return addDays(cursor, direction);
		case 'week':
			return addDays(cursor, 7 * direction);
		case 'month':
			return new Date(cursor.getFullYear(), cursor.getMonth() + direction, cursor.getDate());
		case 'agenda':
			return addDays(cursor, 30 * direction);
	}
}
