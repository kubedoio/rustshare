import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';

test('Calendar opens the same durable meeting note on repeat', async ({ page }) => {
	test.skip(!email || !password, 'ADMIN_EMAIL and ADMIN_PASSWORD are required');
	test.setTimeout(90_000);

	await page.goto('/login');
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files');

	const csrf = (await page.context().cookies()).find(
		(cookie) => cookie.name === 'rustshare_csrf_token'
	)?.value;
	expect(csrf).toBeTruthy();
	const headers = { 'X-Rustshare-Csrf': decodeURIComponent(csrf!) };
	const api = page.context().request;
	const enable = await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', {
		headers
	});
	expect(enable.ok()).toBeTruthy();

	const startsAt = new Date(Date.now() + 24 * 60 * 60 * 1000);
	const title = `H2 meeting ${crypto.randomUUID()}`;
	const created = await api.post('/api/v1/calendar/events', {
		headers,
		data: {
			title,
			starts_at: startsAt.toISOString(),
			ends_at: new Date(startsAt.getTime() + 60 * 60 * 1000).toISOString(),
			timezone: 'UTC',
			description: 'Browser acceptance agenda'
		}
	});
	expect(created.status()).toBe(201);
	const calendarEvent = await created.json();
	let meetingId: string | undefined;
	try {
		await page.goto(`/apps/calendar?event_id=${calendarEvent.id}`);
		await expect(page.getByRole('dialog', { name: 'Event details' })).toBeVisible();
		const openResponse = page.waitForResponse(
			(response) =>
				response.url().endsWith(`/api/v1/calendar/events/${calendarEvent.id}/meeting`) &&
				response.request().method() === 'POST'
		);
		await page.getByRole('button', { name: 'Open meeting workspace', exact: true }).click();
		const opened = await openResponse;
		expect(opened.ok()).toBeTruthy();
		meetingId = (await opened.json()).id;
		await page.waitForURL(/\/apps\/meetings\/[^/]+$/);
		expect(meetingId).toBeTruthy();
		expect(new URL(page.url()).pathname).toBe(`/apps/meetings/${meetingId}`);

		const reopened = await api.post(`/api/v1/calendar/events/${calendarEvent.id}/meeting`, {
			headers
		});
		expect(reopened.ok()).toBeTruthy();
		expect((await reopened.json()).id).toBe(meetingId);
		const note = await api.get(`/api/v1/meetings/${meetingId}`, { headers });
		expect(note.ok()).toBeTruthy();
		expect((await note.json()).content).toContain('Browser acceptance agenda');
	} finally {
		try {
			if (meetingId) {
				const deletedMeeting = await api.delete(`/api/v1/meetings/${meetingId}`, { headers });
				expect(deletedMeeting.ok()).toBeTruthy();
			}
		} finally {
			const deletedEvent = await api.delete(`/api/v1/calendar/events/${calendarEvent.id}`, {
				headers
			});
			expect(deletedEvent.ok()).toBeTruthy();
		}
	}
});
