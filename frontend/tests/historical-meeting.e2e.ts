import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';
const meetingQuery = process.env.PILOT_MEETING_QUERY ?? '';
const meetingTitle = process.env.PILOT_MEETING_TITLE ?? '';
const calendarTitle = process.env.PILOT_CALENDAR_TITLE ?? '';

test('historical meeting search reopens its meeting and originating Calendar event', async ({
	page
}) => {
	test.skip(
		!email || !password || !meetingQuery || !meetingTitle || !calendarTitle,
		'Pilot credentials and historical meeting fixture are required'
	);
	test.setTimeout(60_000);
	page.setDefaultTimeout(15_000);

	await page.goto('/login');
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files', { timeout: 10_000 });

	const search = page.getByPlaceholder('Search files, folders, or activity...');
	await search.fill(meetingQuery);
	const meetingResult = page.getByRole('button', { name: new RegExp(meetingTitle) });
	await expect(meetingResult).toBeVisible();
	await meetingResult.click();

	await expect(page.getByRole('heading', { name: meetingTitle })).toBeVisible();
	const calendarLink = page.getByRole('link', { name: 'Calendar event' });
	await expect(calendarLink).toBeVisible();
	await calendarLink.click();
	await expect(page.getByText(calendarTitle, { exact: true })).toBeVisible();
});
