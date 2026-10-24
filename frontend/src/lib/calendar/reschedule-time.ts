type LocalDateTime = { date: string; time: string };
const DAY_MS = 24 * 60 * 60 * 1000;

const formatterCache = new Map<string, Intl.DateTimeFormat>();

function formatter(timeZone: string): Intl.DateTimeFormat {
	let value = formatterCache.get(timeZone);
	if (!value) {
		value = new Intl.DateTimeFormat('en-CA', {
			timeZone,
			year: 'numeric',
			month: '2-digit',
			day: '2-digit',
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			hourCycle: 'h23'
		});
		formatterCache.set(timeZone, value);
	}
	return value;
}

function partsAt(instant: number, timeZone: string): LocalDateTime {
	const values = Object.fromEntries(
		formatter(timeZone)
			.formatToParts(new Date(instant))
			.map(({ type, value }) => [type, value])
	);
	return {
		date: `${values.year}-${values.month}-${values.day}`,
		time: `${values.hour}:${values.minute}`
	};
}

function localInstant(date: string, time: string, timeZone: string): number {
	const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
	const clock = /^(\d{2}):(\d{2})$/.exec(time);
	if (!match || !clock) throw new RangeError('Choose a valid date and time.');
	const [, year, month, day] = match;
	const [, hour, minute] = clock;
	const desired = Date.UTC(+year, +month - 1, +day, +hour, +minute);
	const check = new Date(desired);
	if (
		check.getUTCFullYear() !== +year ||
		check.getUTCMonth() + 1 !== +month ||
		check.getUTCDate() !== +day ||
		+hour > 23 ||
		+minute > 59
	)
		throw new RangeError('Choose a valid date and time.');

	const offsets = new Set<number>();
	for (let hours = -36; hours <= 36; hours += 6) {
		const instant = desired + hours * 60 * 60 * 1000;
		const local = partsAt(instant, timeZone);
		const [localYear, localMonth, localDay] = local.date.split('-').map(Number);
		const [localHour, localMinute] = local.time.split(':').map(Number);
		offsets.add(Date.UTC(localYear, localMonth - 1, localDay, localHour, localMinute) - instant);
	}

	const matches = [...offsets]
		.map((offset) => desired - offset)
		.filter((instant) => {
			const local = partsAt(instant, timeZone);
			return local.date === date && local.time === time;
		})
		.sort((left, right) => left - right);
	if (matches.length === 0)
		throw new RangeError('That local time does not exist because of a clock change.');
	// During a fall-back overlap, choose the earlier occurrence of the wall time.
	return matches[0];
}

export function localDateTime(instant: string, timeZone: string): LocalDateTime {
	return partsAt(new Date(instant).getTime(), timeZone);
}

export function eventDateForCalendarDate(
	instant: string,
	eventTimeZone: string,
	calendarDate: string,
	calendarTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone
): string {
	const eventDate = localDateTime(instant, eventTimeZone).date;
	const displayedDate = localDateTime(instant, calendarTimeZone).date;
	const dayShift =
		(Date.parse(`${calendarDate}T00:00:00Z`) - Date.parse(`${displayedDate}T00:00:00Z`)) / DAY_MS;
	if (!Number.isInteger(dayShift)) throw new RangeError('Choose a valid calendar date.');
	const eventMidnight = Date.parse(`${eventDate}T00:00:00Z`);
	return new Date(eventMidnight + dayShift * DAY_MS).toISOString().slice(0, 10);
}

export function rescheduleTimedEvent(
	event: { starts_at: string; ends_at: string; all_day: boolean },
	timeZone: string,
	targetDate: string,
	targetTime?: string
): { starts_at: string; ends_at: string } {
	if (event.all_day) throw new RangeError('All-day events cannot be rescheduled.');
	const start = new Date(event.starts_at).getTime();
	const end = new Date(event.ends_at).getTime();
	if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start)
		throw new RangeError('The event must have a positive duration.');
	const original = partsAt(start, timeZone);
	const movedStart = localInstant(targetDate, targetTime ?? original.time, timeZone);
	return {
		starts_at: new Date(movedStart).toISOString(),
		ends_at: new Date(movedStart + end - start).toISOString()
	};
}
