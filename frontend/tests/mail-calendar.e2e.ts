import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';

test.use({ timezoneId: 'Europe/Berlin', hasTouch: true });

test('create a Calendar event from saved Mail and navigate both ways', async ({ page }) => {
	test.skip(!email || !password, 'ADMIN_EMAIL and ADMIN_PASSWORD are required; no IMAP is used');
	test.setTimeout(90_000);
	await page.setViewportSize({ width: 390, height: 844 });
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
	for (const app of ['mail', 'calendar']) {
		const enabled = await api.post(`/api/v1/admin/applications/io.elembra.${app}/enable`, {
			headers
		});
		expect(enabled.ok()).toBeTruthy();
	}
	const subject = `Mail calendar ${crypto.randomUUID()}`;
	const uploaded = await api.post('/api/v1/mail/upload', {
		headers,
		multipart: {
			file: {
				name: 'mail-calendar-test.eml',
				mimeType: 'message/rfc822',
				buffer: Buffer.from(
					`From: sender@example.test\r\nTo: owner@example.test\r\nSubject: ${subject}\r\nDate: Tue, 27 Oct 2026 10:00:00 +0000\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPrivate body that must not enter Calendar.`,
					'utf8'
				)
			}
		}
	});
	expect(uploaded.ok()).toBeTruthy();
	const message = await uploaded.json();
	let eventId: string | null = null;
	let linkId: string | null = null;
	let retryEventId: string | null = null;
	let retryLinkId: string | null = null;
	try {
		await page.goto(`/apps/mail/messages/${message.id}`);
		const create = page.getByRole('button', { name: 'Create Calendar Event' });
		await create.focus();
		await page.keyboard.press('Enter');
		const dialog = page.getByRole('dialog', { name: 'Create Calendar Event' });
		await expect(dialog).toBeVisible();
		await page.getByLabel('Title', { exact: true }).fill('Confirmed appointment');
		const eventRequests: string[] = [];
		page.on('request', (request) => {
			if (request.method() === 'POST' && request.url().endsWith('/api/v1/calendar/events')) {
				eventRequests.push(request.url());
			}
		});
		await page.getByRole('button', { name: 'Cancel', exact: true }).focus();
		await page.keyboard.press('Enter');
		await expect(dialog).toBeHidden();
		expect(eventRequests).toHaveLength(0);

		await create.focus();
		await page.keyboard.press('Enter');
		await page.getByLabel('Title', { exact: true }).fill('Confirmed appointment');
		await page.getByLabel('Start date and time').fill('2026-10-27T12:00');
		await page.getByLabel('End date and time').fill('2026-10-27T13:00');
		await expect(page.getByLabel('Event time zone')).toHaveValue('Europe/Berlin');
		await page.getByLabel('Location (optional)').fill('Room 4');
		const created = page.waitForResponse(
			(response) =>
				response.url().endsWith('/api/v1/calendar/mail-links') &&
				response.request().method() === 'POST'
		);
		await page.getByRole('button', { name: 'Create event', exact: true }).click();
		const linkResponse = await created;
		expect(linkResponse.ok()).toBeTruthy();
		const link = await linkResponse.json();
		linkId = link.id;
		await expect(page.getByText('Calendar event created and linked.')).toBeVisible();
		await expect(page.getByRole('button', { name: 'Open Calendar Event' }).first()).toBeVisible();
		const list = await api.get(`/api/v1/calendar/mail-links?message_id=${message.id}`, { headers });
		expect(list.ok()).toBeTruthy();
		const related = await list.json();
		eventId = related[0].event_id;
		const event = await (await api.get(`/api/v1/calendar/events/${eventId}`, { headers })).json();
		expect(event.title).toBe('Confirmed appointment');
		expect(event.description).toBeNull();

		await page.getByRole('button', { name: 'Open Calendar Event' }).first().click();
		await expect(page).toHaveURL(new RegExp(`/apps/calendar\\?event_id=${eventId}`));
		await page.getByRole('button', { name: 'Related Mail' }).click();
		await expect(page).toHaveURL(new RegExp(`/apps/mail/messages/${message.id}`));
		await expect(page.getByRole('heading', { name: subject })).toBeVisible();

		let linkAttempts = 0;
		await page.route('**/api/v1/calendar/mail-links', async (route) => {
			if (route.request().method() === 'POST' && linkAttempts++ === 0) {
				await route.fulfill({ status: 503, json: { error: 'Temporary link failure' } });
			} else {
				await route.continue();
			}
		});
		await page.getByRole('button', { name: 'Create Calendar Event' }).click();
		await page.getByLabel('Title', { exact: true }).fill('Retry appointment');
		await page.getByLabel('Start date and time').fill('2026-10-28T12:00');
		await page.getByLabel('End date and time').fill('2026-10-28T13:00');
		const eventCreated = page.waitForResponse(
			(response) =>
				response.url().endsWith('/api/v1/calendar/events') && response.request().method() === 'POST'
		);
		const linkFailed = page.waitForResponse(
			(response) =>
				response.url().endsWith('/api/v1/calendar/mail-links') &&
				response.request().method() === 'POST'
		);
		await page.getByRole('button', { name: 'Create event', exact: true }).click();
		const partialEventResponse = await eventCreated;
		expect(partialEventResponse.status()).toBe(201);
		retryEventId = (await partialEventResponse.json()).id;
		expect((await linkFailed).status()).toBe(503);
		const retryDialog = page.getByRole('dialog', { name: 'Finish linking event' });
		await expect(retryDialog.getByRole('alert')).toContainText('event was created');
		const retried = page.waitForResponse(
			(response) =>
				response.url().endsWith('/api/v1/calendar/mail-links') &&
				response.request().method() === 'POST'
		);
		await retryDialog.getByRole('button', { name: 'Retry Mail link' }).click();
		const retryResponse = await retried;
		expect(retryResponse.ok()).toBeTruthy();
		retryLinkId = (await retryResponse.json()).id;
		await expect(page.getByText('Calendar event created and linked.')).toBeVisible();
	} finally {
		if (linkId) await api.delete(`/api/v1/calendar/mail-links/${linkId}`, { headers });
		if (retryLinkId) await api.delete(`/api/v1/calendar/mail-links/${retryLinkId}`, { headers });
		if (eventId) await api.delete(`/api/v1/calendar/events/${eventId}`, { headers });
		if (retryEventId) await api.delete(`/api/v1/calendar/events/${retryEventId}`, { headers });
	}
});
