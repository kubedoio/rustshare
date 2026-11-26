import { expect, test } from '@playwright/test';

const email = process.env.ADMIN_EMAIL ?? '';
const password = process.env.ADMIN_PASSWORD ?? '';
const noteTitle = process.env.PILOT_NOTE_TITLE ?? '';
const noteId = process.env.PILOT_NOTE_ID ?? '';
const fileName = process.env.PILOT_FILE_NAME ?? '';

async function signIn(page: import('@playwright/test').Page, email: string, password: string) {
	await page.goto('/login');
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files');
}

async function csrfHeaders(context: import('@playwright/test').BrowserContext) {
	const csrf = (await context.cookies()).find(
		(cookie) => cookie.name === 'rustshare_csrf_token'
	)?.value;
	expect(csrf).toBeTruthy();
	return { 'X-Rustshare-Csrf': decodeURIComponent(csrf!) };
}

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
	await signIn(page, email, password);
	const headers = await csrfHeaders(page.context());
	const api = page.context().request;
	const enabled = await api.post('/api/v1/admin/applications/io.elembra.calendar/enable', {
		headers
	});
	expect(enabled.ok()).toBeTruthy();
	const profileResponse = await api.get('/api/v1/me');
	expect(profileResponse.ok()).toBeTruthy();
	const profile = await profileResponse.json();

	// Exercise the production operator bootstrap, tenant grants, and tenant-scoped group APIs.
	// Each created principal is synthetic and confined to this disposable pilot database.
	const syntheticPassword = `Pilot-${suffix}!`;
	const createPrincipal = async (kind: string) => {
		const response = await api.post('/api/v1/admin/users', {
			headers,
			data: {
				username: `pilot-${kind}-${suffix}`,
				email: `pilot-${kind}-${suffix}@example.com`,
				password: syntheticPassword,
				display_name: `Pilot ${kind} ${suffix}`,
				is_admin: false
			}
		});
		expect(response.status()).toBe(201);
		return response.json();
	};
	const orgAdminA = await createPrincipal('org-a');
	const orgAdminB = await createPrincipal('org-b');
	const ordinary = await createPrincipal('member');
	for (const principal of [orgAdminA, orgAdminB]) {
		const granted = await api.post(
			`/api/v1/admin/organizations/${profile.tenant_id}/administrators/${principal.id}`,
			{ headers, data: {} }
		);
		expect(granted.status()).toBe(201);
	}

	const orgContextA = await browser.newContext();
	const orgContextB = await browser.newContext();
	const memberContext = await browser.newContext();
	try {
		const orgPageA = await orgContextA.newPage();
		const orgPageB = await orgContextB.newPage();
		const memberPage = await memberContext.newPage();
		await signIn(orgPageA, orgAdminA.email, syntheticPassword);
		await signIn(orgPageB, orgAdminB.email, syntheticPassword);
		await signIn(memberPage, ordinary.email, syntheticPassword);
		const orgApiA = orgContextA.request;
		const orgApiB = orgContextB.request;
		const memberApi = memberContext.request;
		const orgHeadersA = await csrfHeaders(orgContextA);
		const orgHeadersB = await csrfHeaders(orgContextB);
		const groupResponse = await orgApiA.post('/api/v1/organization-admin/groups', {
			headers: orgHeadersA,
			data: {
				name: `Pilot Group ${suffix}`,
				description: 'Synthetic institutional acceptance group'
			}
		});
		expect(groupResponse.status()).toBe(201);
		const group = await groupResponse.json();
		await orgPageA.goto('/organization-admin/groups');
		await expect(orgPageA.getByText(`Pilot Group ${suffix}`, { exact: true })).toBeVisible();

		// A separate granted administrator can manage membership; the ordinary member cannot.
		const groupList = await orgApiB.get('/api/v1/organization-admin/groups');
		expect(groupList.ok()).toBeTruthy();
		for (const principal of [orgAdminB, ordinary]) {
			const added = await orgApiB.post(`/api/v1/organization-admin/groups/${group.id}/members`, {
				headers: orgHeadersB,
				data: { user_id: principal.id }
			});
			expect(added.status()).toBe(204);
		}
		const deniedManagement = await memberApi.get('/api/v1/organization-admin/groups');
		expect(deniedManagement.status()).toBe(403);

		// Group access works while membership is active and is revoked immediately on removal.
		const sharedNoteResponse = await orgApiB.post('/api/v1/notes', {
			headers: orgHeadersB,
			data: { title: `Shared pilot note ${suffix}`, content: 'Synthetic group access evidence' }
		});
		expect(sharedNoteResponse.status()).toBe(201);
		const sharedNote = await sharedNoteResponse.json();
		const shareResponse = await orgApiB.post(`/api/v1/files/${sharedNote.id}/share/group`, {
			headers: orgHeadersB,
			data: { group_id: group.id, permission: 'View' }
		});
		expect(shareResponse.status()).toBe(201);
		const memberAccess = await memberApi.get(`/api/v1/notes/${sharedNote.id}`);
		expect(memberAccess.ok()).toBeTruthy();
		expect((await memberAccess.json()).content).toContain('Synthetic group access evidence');
		const removed = await orgApiB.delete(
			`/api/v1/organization-admin/groups/${group.id}/members/${ordinary.id}`,
			{ headers: orgHeadersB }
		);
		expect(removed.status()).toBe(204);
		const revokedAccess = await memberApi.get(`/api/v1/notes/${sharedNote.id}`);
		expect([403, 404]).toContain(revokedAccess.status());
		expect(await revokedAccess.text()).not.toContain('Synthetic group access evidence');

		const auditResponse = await orgApiA.get('/api/v1/organization-admin/audit');
		expect(auditResponse.ok()).toBeTruthy();
		const audit = await auditResponse.json();
		const groupActions = audit.entries.filter(
			(entry: { target_id: string }) => entry.target_id === group.id
		);
		expect(groupActions.map((entry: { action_type: string }) => entry.action_type)).toEqual(
			expect.arrayContaining(['group.created', 'group.member_added', 'group.member_removed'])
		);
		expect(
			groupActions.some((entry: { actor_id: string }) => entry.actor_id === orgAdminB.id)
		).toBeTruthy();
	} finally {
		await Promise.all([orgContextA.close(), orgContextB.close(), memberContext.close()]);
	}

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
	await page.getByRole('button', { name: 'Create/Open Meeting Notes', exact: true }).click();
	await expect(page).toHaveURL(/\/apps\/notes\/[\da-f-]+\?/i);
	const eventNoteId = new URL(page.url()).pathname.split('/').pop()!;
	const eventNoteResponse = await api.get(`/api/v1/notes/${eventNoteId}`);
	expect(eventNoteResponse.ok()).toBeTruthy();
	expect((await eventNoteResponse.json()).content).toContain('Synthetic Gate A discussion');
	await expect(page.getByRole('button', { name: 'Back to calendar event' })).toBeVisible();
	await page.getByRole('button', { name: 'Back to calendar event' }).click();
	await expect(page).toHaveURL(new RegExp(`/apps/calendar\\?event_id=${event.id}`));
	const reopenedEventNote = await api.post(`/api/v1/calendar/events/${event.id}/notes`, {
		headers
	});
	expect(reopenedEventNote.ok()).toBeTruthy();
	expect((await reopenedEventNote.json()).note_id).toBe(eventNoteId);
	await page.getByRole('button', { name: 'Open meeting workspace', exact: true }).click();
	await expect(page).toHaveURL(/\/apps\/meetings\/[\da-f-]+$/);
	const meetingId = page.url().split('/').pop()!;
	const resources = page.getByRole('region', { name: 'Linked resources', exact: true });
	const linkedUrls: string[] = [];
	const eventNoteLink = await api.post(`/api/v1/meetings/${meetingId}/resources`, {
		headers,
		data: {
			resource_ref: `elembra://io.elembra.notes/notes.resource/${eventNoteId}`
		}
	});
	expect(eventNoteLink.ok()).toBeTruthy();
	const linkedEventNote = await eventNoteLink.json();
	expect(linkedEventNote.open_url).toBe(`/apps/notes/${eventNoteId}`);
	linkedUrls.push(linkedEventNote.open_url);
	await page.reload();
	await expect(resources.locator(`a[href="${linkedEventNote.open_url}"]`)).toBeVisible();
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
	for (const [index, link] of [
		resources.locator(`a[href="/apps/notes/${noteId}"]`),
		resources.locator('a[href^="/files?preview="]')
	].entries()) {
		await link.click();
		await expect(page).not.toHaveURL(new RegExp(`/apps/meetings/${meetingId}$`));
		if (index === 0) {
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
	await page
		.getByRole('listbox')
		.getByRole('option', { name: new RegExp(`^${title} #`) })
		.click();
	await expect(page).toHaveURL(new RegExp(`/apps/meetings/${meetingId}$`));
	await page.reload();
	await expect(page.locator('main.doc-content')).toContainText('Synthetic Gate A discussion');
	await expect(resources.locator('a')).toHaveCount(3);
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

	// A deleted link must expose only its sanitized state, without title or URL.
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
	await expect(resources.getByText('Linked resource deleted', { exact: true })).toBeVisible();
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
