import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';
const noteTitle = process.env.PILOT_NOTE_TITLE ?? '';
const noteId = process.env.PILOT_NOTE_ID ?? '';
const fileName = process.env.PILOT_FILE_NAME ?? '';

test('institutional Calendar journey preserves linked context and denies unavailable content', async ({
	page,
	browser
}) => {
	test.setTimeout(180_000);
	page.setDefaultTimeout(15_000);
	// This creates synthetic records and deletes only its own stale-link fixture.
	expect(process.env.PILOT_ACCEPTANCE_MUTATIONS).toBe('authorized');
	expect(email && password && noteTitle && noteId && fileName).toBeTruthy();
	const suffix = crypto.randomUUID();
	const title = `GateA-${suffix}`;
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
	const enabled = await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', {
		headers
	});
	expect(enabled.ok()).toBeTruthy();
	const profileResponse = await api.get('/api/v1/me');
	expect(profileResponse.ok()).toBeTruthy();
	const profile = await profileResponse.json();
	const noteResponse = await api.get(`/api/v1/notes/${noteId}`);
	expect(noteResponse.ok()).toBeTruthy();
	const note = await noteResponse.json();

	await page.goto('/apps/calendar');
	await page.getByRole('button', { name: 'New event', exact: true }).click();
	await page.getByRole('textbox', { name: 'Event title', exact: true }).fill(title);
	const yesterday = new Date();
	yesterday.setDate(yesterday.getDate() - 1);
	await page
		.locator('#cal-event-date')
		.fill(
			`${yesterday.getFullYear()}-${String(yesterday.getMonth() + 1).padStart(2, '0')}-${String(yesterday.getDate()).padStart(2, '0')}`
		);
	await page.locator('#cal-event-start').fill('10:00');
	await page.locator('#cal-event-end').fill('11:00');
	await page
		.getByRole('textbox', { name: 'Description', exact: true })
		.fill('Synthetic Gate A discussion');
	const created = page.waitForResponse(
		(response) =>
			response.url().endsWith('/api/v1/calendar/events') && response.request().method() === 'POST'
	);
	await page.getByRole('button', { name: 'Create event', exact: true }).click();
	const eventResponse = await created;
	expect(eventResponse.status()).toBe(201);
	const event = await eventResponse.json();
	await page.goto(`/apps/calendar?event_id=${event.id}`);
	await page.getByRole('button', { name: 'Open meeting note', exact: true }).click();
	await expect(page).toHaveURL(/\/apps\/meetings\/[\da-f-]+$/);
	const meetingId = page.url().split('/').pop()!;
	const resources = page.getByRole('region', { name: 'Linked resources', exact: true });
	const linkedUrls: string[] = [];
	await resources.getByRole('button', { name: 'Link a resource' }).click();
	for (const [name, identity] of [
		[noteTitle, note.path],
		[fileName, fileName]
	]) {
		await resources.getByRole('textbox', { name: 'Search accessible Files and Notes' }).fill(name);
		await resources.getByRole('button', { name: 'Search', exact: true }).click();
		const candidate = resources
			.getByRole('listitem')
			.filter({ has: page.getByText(identity, { exact: true }) })
			.filter({ has: page.getByRole('button', { name: 'Link', exact: true }) });
		const linked = page.waitForResponse(
			(response) =>
				response.url().endsWith(`/api/v1/meetings/${meetingId}/resources`) &&
				response.request().method() === 'POST'
		);
		await candidate.getByRole('button', { name: 'Link', exact: true }).click();
		const linkResponse = await linked;
		expect(linkResponse.ok()).toBeTruthy();
		const link = await linkResponse.json();
		if (name === noteTitle) expect(link.open_url).toBe(`/apps/notes/${noteId}`);
		else expect(link.resource_kind).toBe('file');
		linkedUrls.push(link.open_url);
		await expect(resources.locator(`a[href="${link.open_url}"]`)).toBeVisible();
	}
	await resources.getByRole('button', { name: 'Close', exact: true }).click();
	for (const path of ['/apps/notes/', '/files?preview=']) {
		await resources.locator(`a[href^="${path}"]`).click();
		await expect(page).not.toHaveURL(new RegExp(`/apps/meetings/${meetingId}$`));
		if (path === '/apps/notes/') {
			await expect(page.locator('.doc-subtitle')).toHaveText('Beta Smoke H1');
		} else {
			await expect(page.getByRole('heading', { name: fileName, exact: true })).toBeVisible();
		}
		await page.goBack();
		await expect(page.getByRole('heading', { name: title, exact: true })).toBeVisible();
	}
	const decisions = page.getByRole('region', { name: 'Meeting decisions' });
	await decisions.getByRole('button', { name: 'Create Decision', exact: true }).click();
	await decisions.getByLabel('Decision title', { exact: true }).fill(`Decision ${suffix}`);
	await decisions
		.getByLabel('Decision content', { exact: true })
		.fill('Synthetic approved outcome');
	await decisions.getByRole('button', { name: 'Create decision', exact: true }).click();
	await expect(
		decisions.getByRole('link', { name: `Decision ${suffix}`, exact: true })
	).toBeVisible();
	const actions = page.getByRole('region', { name: 'Meeting actions' });
	await actions.getByRole('button', { name: 'Create action', exact: true }).click();
	await actions.getByLabel('Action title', { exact: true }).fill(`Action ${suffix}`);
	await actions.getByLabel('Responsible person').selectOption(profile.id);
	await actions.getByLabel('Related Decision').selectOption({ label: `Decision ${suffix}` });
	await actions.getByRole('button', { name: 'Save new action', exact: true }).click();
	await expect(actions.getByText('Open · Responsible:', { exact: false })).toBeVisible();

	await page.getByRole('button', { name: 'Calendar event', exact: true }).click();
	await expect(page.getByRole('dialog', { name: 'Event details', exact: true })).toContainText(
		title
	);
	await page.getByRole('button', { name: 'Close event details' }).click();
	await page.getByPlaceholder('Search files, folders, or activity...').fill(title);
	await page.getByRole('option').filter({ hasText: title }).click();
	await expect(page).toHaveURL(new RegExp(`/apps/meetings/${meetingId}$`));
	await page.reload();
	await expect(page.locator('main.doc-content')).toContainText('Synthetic Gate A discussion');
	await expect(resources.locator('a')).toHaveCount(2);
	for (const url of linkedUrls) await expect(resources.locator(`a[href="${url}"]`)).toBeVisible();
	await expect(
		decisions.getByRole('link', { name: `Decision ${suffix}`, exact: true })
	).toBeVisible();
	await expect(
		actions.getByRole('heading', { name: `Action ${suffix}`, exact: true })
	).toBeVisible();
	const savedActionsResponse = await api.get(`/api/v1/meetings/${meetingId}/actions`);
	expect(savedActionsResponse.ok()).toBeTruthy();
	const savedAction = (await savedActionsResponse.json()).items[0];
	expect(savedAction).toMatchObject({
		assignee_id: profile.id,
		state: 'open',
		calendar_event_id: event.id
	});
	const savedDecisionsResponse = await api.get(`/api/v1/meetings/${meetingId}/decisions`);
	expect(savedDecisionsResponse.ok()).toBeTruthy();
	const decision = (await savedDecisionsResponse.json()).items[0].decision;
	expect(decision.owner_id).toBe(profile.id);
	expect(decision.status).toBe('proposed');

	// A stale link must retain an unavailable placeholder without title or URL.
	const staleResponse = await api.post('/api/v1/notes', {
		headers,
		data: { title: `Stale ${suffix}`, content: 'Synthetic unavailable context' }
	});
	expect(staleResponse.ok()).toBeTruthy();
	const stale = await staleResponse.json();
	const linked = await api.post(`/api/v1/meetings/${meetingId}/resources`, {
		headers,
		data: { resource_ref: `elembra://io.elembra.files/file/${stale.id}` }
	});
	expect(linked.ok()).toBeTruthy();
	expect((await api.delete(`/api/v1/notes/${stale.id}`, { headers })).ok()).toBeTruthy();
	await page.reload();
	await expect(resources.getByText('Linked resource unavailable', { exact: true })).toBeVisible();
	await expect(resources).not.toContainText(`Stale ${suffix}`);
	const anonymous = await browser.newContext({ baseURL: new URL(page.url()).origin });
	try {
		for (const path of [
			`meetings/${meetingId}`,
			`calendar/events/${event.id}`,
			`notes/${stale.id}`
		]) {
			const denied = await anonymous.request.get(`/api/v1/${path}`);
			expect(denied.status()).toBe(401);
			expect(await denied.text()).not.toContain(title);
		}
	} finally {
		await anonymous.close();
	}
});
