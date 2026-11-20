import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';

test('Calendar creates durable Notes and the Note links back to its event', async ({ page }) => {
	test.setTimeout(90_000);
	expect(email && password).toBeTruthy();
	const suffix = crypto.randomUUID();
	const title = `Meeting Notes ${suffix}`;
	let eventId: string | null = null;
	let noteId: string | null = null;
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
	const wasEnabled = (await appResponse.json()).enabled;
	const enable = await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', {
		headers
	});
	expect(enable.ok()).toBeTruthy();
	restoreCalendarState = !wasEnabled;

	try {
		const created = await api.post('/api/v1/calendar/events', {
			headers,
			data: {
				title,
				description: 'Agenda from the Calendar event',
				starts_at: new Date(Date.now() + 60 * 60 * 1000).toISOString(),
				ends_at: new Date(Date.now() + 2 * 60 * 60 * 1000).toISOString(),
				timezone: 'UTC'
			}
		});
		expect(created.ok()).toBeTruthy();
		eventId = (await created.json()).id;

		await page.goto(`/apps/calendar?event_id=${eventId}`);
		await page.getByRole('button', { name: 'Create/Open Meeting Notes' }).click();
		await expect(page).toHaveURL(/\/apps\/notes\/[0-9a-f-]+/i);
		noteId = new URL(page.url()).pathname.split('/').at(-1)!;
		await expect(page.getByRole('button', { name: 'Back to calendar event' })).toBeVisible();
		await page.getByRole('button', { name: 'Back to calendar event' }).click();
		await expect(page).toHaveURL(new RegExp(`/apps/calendar\\?event_id=${eventId}`));

		const reopened = await api.post(`/api/v1/calendar/events/${eventId}/notes`, {
			headers
		});
		expect(reopened.ok()).toBeTruthy();
		expect((await reopened.json()).note_id).toBe(noteId);

		const deletedEvent = await api.delete(`/api/v1/calendar/events/${eventId}`, { headers });
		expect(deletedEvent.ok()).toBeTruthy();
		const preservedNote = await api.get(`/api/v1/notes/${noteId}`);
		expect(preservedNote.ok()).toBeTruthy();
		expect((await preservedNote.json()).content).toContain('Agenda from the Calendar event');
	} finally {
		if (noteId) await api.delete(`/api/v1/notes/${noteId}`, { headers });
		if (eventId) await api.delete(`/api/v1/calendar/events/${eventId}`, { headers });
		if (restoreCalendarState) {
			await api.post('/api/v1/admin/applications/io.elembra.calendar/disable', {
				headers
			});
		}
	}
});
