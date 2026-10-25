import { expect, test, type Page } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';

test.use({ timezoneId: 'America/Los_Angeles', hasTouch: true });

async function createInternalEvent(page: Page, title: string) {
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
	expect(
		(await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', { headers })).ok()
	).toBeTruthy();
	const date = await page.evaluate(() => {
		const day = new Date();
		day.setDate(day.getDate() + 3);
		return `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, '0')}-${String(day.getDate()).padStart(2, '0')}`;
	});
	const start = new Date(`${date}T10:00:00Z`);
	const created = await api.post('/api/v1/calendar/events', {
		headers,
		data: {
			title,
			starts_at: start.toISOString(),
			ends_at: new Date(start.getTime() + 60 * 60 * 1000).toISOString(),
			timezone: 'Europe/Berlin'
		}
	});
	expect(created.status()).toBe(201);
	return { api, headers, date, event: await created.json() };
}

const viewports = [
	{ name: 'desktop', width: 1280, height: 900 },
	{ name: 'narrow', width: 390, height: 844 }
];

for (const viewport of viewports) {
	test(`Calendar move succeeds on ${viewport.name} viewport`, async ({ page }) => {
		test.skip(
			!email || !password,
			'ADMIN_EMAIL and ADMIN_PASSWORD are required; no provider credentials are used'
		);
		test.setTimeout(90_000);
		await page.setViewportSize(viewport);
		const fixture = await createInternalEvent(page, `M2.2 move ${crypto.randomUUID()}`);
		try {
			const target = new Date(`${fixture.date}T12:00:00Z`);
			target.setUTCDate(target.getUTCDate() + 1);
			const targetDate = target.toISOString().slice(0, 10);
			if (viewport.width > 700) {
				await page.goto('/apps/calendar');
				const source = page.getByRole('button', { name: new RegExp(fixture.event.title) });
				await source.dragTo(page.locator(`[data-calendar-date="${targetDate}"]`));
			} else {
				await page.goto(`/apps/calendar?event_id=${fixture.event.id}`);
				await page.getByRole('button', { name: /Move \/ reschedule/ }).tap();
				await page.getByLabel('New date').fill(targetDate);
			}
			await expect(page.getByRole('dialog', { name: 'Move event' })).toBeVisible();
			const expectedTime = new Intl.DateTimeFormat('en-GB', {
				timeZone: fixture.event.timezone,
				hour: '2-digit',
				minute: '2-digit',
				hourCycle: 'h23'
			}).format(new Date(fixture.event.starts_at));
			await expect(page.getByLabel('New start time')).toHaveValue(expectedTime);
			const response = page.waitForResponse(
				(res) =>
					res.url().endsWith(`/api/v1/calendar/events/${fixture.event.id}`) &&
					res.request().method() === 'PATCH'
			);
			await page.getByRole('button', { name: 'Confirm move' }).click();
			expect((await response).ok()).toBeTruthy();
			await expect(page.getByText('Event moved', { exact: true })).toBeVisible();
		} finally {
			await fixture.api.delete(`/api/v1/calendar/events/${fixture.event.id}`, {
				headers: fixture.headers
			});
		}
	});

	test(`Calendar move cancels on ${viewport.name} viewport`, async ({ page }) => {
		test.skip(
			!email || !password,
			'ADMIN_EMAIL and ADMIN_PASSWORD are required; no provider credentials are used'
		);
		test.setTimeout(90_000);
		await page.setViewportSize(viewport);
		const fixture = await createInternalEvent(page, `M2.2 cancel ${crypto.randomUUID()}`);
		try {
			await page.goto(`/apps/calendar?event_id=${fixture.event.id}`);
			const move = page.getByRole('button', { name: /Move \/ reschedule/ });
			if (viewport.width > 700) {
				await move.focus();
				await page.keyboard.press('Enter');
			} else {
				await move.tap();
			}
			await expect(page.getByRole('dialog', { name: 'Move event' })).toBeVisible();
			const requests: string[] = [];
			page.on('request', (request) => {
				if (request.method() === 'PATCH' && request.url().includes('/api/v1/calendar/events/'))
					requests.push(request.url());
			});
			await page.getByLabel('New date').fill(fixture.date);
			const cancel = page.getByRole('button', { name: 'Cancel', exact: true });
			if (viewport.width > 700) {
				await cancel.focus();
				await page.keyboard.press('Enter');
			} else {
				await cancel.tap();
			}
			await expect(page.getByRole('dialog', { name: 'Move event' })).toHaveCount(0);
			expect(requests).toHaveLength(0);
		} finally {
			await fixture.api.delete(`/api/v1/calendar/events/${fixture.event.id}`, {
				headers: fixture.headers
			});
		}
	});
}
