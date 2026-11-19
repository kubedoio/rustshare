import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';

function event(uid: string, title: string, day: string) {
	return `BEGIN:VEVENT\nUID:${uid}\nDTSTAMP:${day}T120000Z\nDTSTART:${day}T150000Z\nDTEND:${day}T160000Z\nSUMMARY:${title}\nEND:VEVENT`;
}

function calendar(events: string[]) {
	return `BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Elembra//H2.1 acceptance//EN\n${events.join('\n')}\nEND:VCALENDAR\n`;
}

test('Calendar Settings previews, confirms, displays, exports, and idempotently re-imports iCalendar events', async ({
	page
}) => {
	test.setTimeout(120_000);
	expect(email && password).toBeTruthy();
	const suffix = crypto.randomUUID();
	const sourceName = `E2E Calendar ${suffix}`;
	const initialFilename = `calendar-initial-${suffix}.ics`;
	const reviewFilename = `calendar-review-${suffix}.ics`;
	const exportedFilename = `calendar-export-${suffix}.ics`;
	const day = new Date().toISOString().slice(0, 10).replaceAll('-', '');
	const firstUid = `calendar-first-${suffix}@example.test`;
	const conflictUid = `calendar-conflict-${suffix}@example.test`;
	const newUid = `calendar-new-${suffix}@example.test`;
	const firstTitle = `Calendar first ${suffix}`;
	const originalConflictTitle = `Calendar conflict original ${suffix}`;
	const changedConflictTitle = `Calendar conflict changed ${suffix}`;
	const newTitle = `Calendar new ${suffix} <script>alert(1)</script>`;
	let sourceId: string | null = null;
	let calendarWasEnabled = true;
	let restoreCalendarState = false;

	await page.goto('/login');
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files');

	const api = page.context().request;
	const csrf = (await page.context().cookies()).find(
		(cookie) => cookie.name === 'rustshare_csrf_token'
	)?.value;
	expect(csrf).toBeTruthy();
	const headers = { 'X-Rustshare-Csrf': decodeURIComponent(csrf!) };
	const appResponse = await api.get('/api/v1/admin/applications/io.elembra.calendar');
	expect(appResponse.ok()).toBeTruthy();
	calendarWasEnabled = (await appResponse.json()).enabled;
	const enabled = await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', {
		headers
	});
	expect(enabled.ok()).toBeTruthy();
	restoreCalendarState = !calendarWasEnabled;
	if (restoreCalendarState) await page.reload();

	const waitForImport = async (filename: string) => {
		await expect
			.poll(
				async () => {
					const response = await api.get('/api/v1/calendar/import-jobs');
					if (!response.ok()) return 'unavailable';
					const { jobs } = await response.json();
					return (
						jobs.find((job: { filename: string }) => job.filename === filename)?.status ?? 'pending'
					);
				},
				{ timeout: 30_000, intervals: [250, 500, 1000] }
			)
			.toBe('completed');
	};

	try {
		await page.goto('/settings/apps/calendar');
		await expect(page.getByRole('heading', { name: 'Calendar settings', level: 1 })).toBeVisible();
		await page.getByLabel('New destination name').fill(sourceName);
		await page.getByRole('button', { name: 'Create destination' }).click();
		const destination = page.getByLabel('Destination calendar');
		await expect(destination.getByRole('option', { name: sourceName })).toBeAttached();
		sourceId = await destination
			.locator('option')
			.filter({ hasText: sourceName })
			.getAttribute('value');
		expect(sourceId).toBeTruthy();
		await destination.selectOption(sourceId!);

		const initial = calendar([
			event(firstUid, firstTitle, day),
			event(conflictUid, originalConflictTitle, day)
		]);
		await page.getByLabel('Choose .ics file').setInputFiles({
			name: initialFilename,
			mimeType: 'text/calendar',
			buffer: Buffer.from(initial)
		});
		await expect(page.getByText('2 new · 0 unchanged · 0 changed · 0 unsupported')).toBeVisible();
		await page.getByRole('button', { name: 'Confirm import' }).click();
		await waitForImport(initialFilename);

		const review = calendar([
			event(firstUid, firstTitle, day),
			event(conflictUid, changedConflictTitle, day),
			event(newUid, newTitle, day),
			`${event(`calendar-unsupported-${suffix}@example.test`, `Unsupported ${suffix}`, day).replace('END:VEVENT', 'ATTACH;VALUE=URI:http://127.0.0.1:9/private.ics\nEND:VEVENT')}`
		]);
		await page.getByLabel('Choose .ics file').setInputFiles({
			name: reviewFilename,
			mimeType: 'text/calendar',
			buffer: Buffer.from(review)
		});
		await expect(page.getByText('1 new · 1 unchanged · 1 changed · 1 unsupported')).toBeVisible();
		await expect(page.getByText('duplicate', { exact: true })).toBeVisible();
		await expect(page.getByText('conflict', { exact: true })).toBeVisible();
		await expect(page.getByText('unsupported', { exact: true })).toBeVisible();
		await page.getByRole('button', { name: 'Confirm import' }).click();
		await waitForImport(reviewFilename);

		await page.goto('/apps/calendar');
		const newEvent = page.getByRole('button', { name: new RegExp(`Calendar new ${suffix}`) });
		await expect(newEvent).toBeVisible();
		await expect(newEvent).toContainText('<script>alert(1)</script>');
		await expect(
			page.getByRole('button', { name: new RegExp(`Calendar first ${suffix}`) })
		).toBeVisible();
		await expect(
			page.getByRole('button', { name: new RegExp(`Calendar conflict changed ${suffix}`) })
		).toBeVisible();
		await newEvent.click();
		const details = page.getByRole('dialog', { name: 'Event details' });
		await expect(details).toContainText(newTitle);

		const downloadPromise = page.waitForEvent('download');
		await details.getByRole('button', { name: 'Export .ics' }).click();
		const download = await downloadPromise;
		const stream = await download.createReadStream();
		if (!stream) throw new Error('Export download stream was unavailable');
		const chunks: Buffer[] = [];
		for await (const chunk of stream) chunks.push(Buffer.from(chunk));

		await page.goto('/settings/apps/calendar');
		await page.getByLabel('Destination calendar').selectOption(sourceId!);
		await page.getByLabel('Choose .ics file').setInputFiles({
			name: exportedFilename,
			mimeType: 'text/calendar',
			buffer: Buffer.concat(chunks)
		});
		await expect(page.getByText('0 new · 1 unchanged · 0 changed · 0 unsupported')).toBeVisible();
		await page.getByRole('button', { name: 'Confirm import' }).click();
		await waitForImport(exportedFilename);

		const events = await api.get(
			`/api/v1/calendar/events?from=${day.slice(0, 4)}-${day.slice(4, 6)}-${day.slice(6)}T00%3A00%3A00Z&to=${day.slice(0, 4)}-${day.slice(4, 6)}-${day.slice(6)}T23%3A59%3A59Z`
		);
		expect(events.ok()).toBeTruthy();
		const { events: storedEvents } = await events.json();
		const imported = storedEvents.filter(
			(event: { source_id: string }) => event.source_id === sourceId
		);
		expect(imported).toHaveLength(3);
		expect(imported.map((event: { title: string }) => event.title)).toContain(changedConflictTitle);
		expect(imported.map((event: { title: string }) => event.title)).toContain(newTitle);
	} finally {
		if (sourceId) await api.delete(`/api/v1/calendar/sources/${sourceId}`, { headers });
		if (restoreCalendarState) {
			await api.post('/api/v1/admin/applications/io.elembra.calendar/disable', { headers });
		}
	}
});
