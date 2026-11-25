import { expect, test, type Page } from '@playwright/test';
import { randomUUID } from 'node:crypto';

const adminEmail = process.env.ADMIN_EMAIL ?? '';
const adminPassword = process.env.ADMIN_PASSWORD ?? '';

type SyntheticUser = {
	id: string;
	username: string;
	email: string;
	displayName: string;
	password: string;
};

async function login(page: Page, email: string, password: string) {
	const loginResponsePromise = page.waitForResponse(
		(response) =>
			response.url().endsWith('/api/v1/auth/login') && response.request().method() === 'POST'
	);
	await page.goto('/login');
	await page.getByLabel('Email').fill(email);
	await page.getByLabel('Password').fill(password);
	await page.getByRole('button', { name: 'Sign in with password' }).click();
	await page.waitForURL('**/files');
	const loginResponse = await loginResponsePromise;
	expect(loginResponse.ok()).toBeTruthy();
	const loginBody = await loginResponse.json();
	const csrfCookie = (await page.context().cookies()).find(
		(cookie) => cookie.name === 'rustshare_csrf_token'
	)?.value;
	expect(csrfCookie).toBeTruthy();
	return {
		api: page.context().request,
		headers: { 'X-Rustshare-Csrf': decodeURIComponent(csrfCookie!) },
		tenantId: loginBody.user.tenant_id as string
	};
}

test('organization admins manage a 100-user synthetic organization safely', async ({
	page,
	browser
}) => {
	test.setTimeout(300_000);
	page.setDefaultTimeout(20_000);
	expect(process.env.PILOT_ACCEPTANCE_MUTATIONS).toBe('authorized');
	expect(adminEmail && adminPassword).toBeTruthy();

	const suffix = randomUUID().replaceAll('-', '').slice(0, 12);
	const usernamePrefix = `p100${suffix}`;
	const emailPrefix = `pilot-scale-${suffix}`;
	const syntheticPassword = `ScaleUser-${randomUUID()}!`;
	const users: SyntheticUser[] = [];
	let tenantId = '';
	let baselineAdminIds: string[] = [];
	const grantedAdminIds: string[] = [];

	const operator = await login(page, adminEmail, adminPassword);
	tenantId = operator.tenantId;
	const adminApi = operator.api;

	try {
		for (let index = 1; index <= 100; index += 1) {
			const number = String(index).padStart(3, '0');
			const username = `${usernamePrefix}${number}`;
			const email = `${emailPrefix}-${number}@example.com`;
			const displayName = `Scale ${number}`;
			const response = await adminApi.post('/api/v1/admin/users', {
				headers: operator.headers,
				data: {
					username,
					email,
					display_name: displayName,
					password: syntheticPassword,
					is_admin: false
				}
			});
			expect(response.status(), `create synthetic account ${number}`).toBe(201);
			const created = await response.json();
			users.push({
				id: created.id,
				username,
				email,
				displayName,
				password: syntheticPassword
			});
		}

		const firstOrganizationAdmin = users[98];
		const secondOrganizationAdmin = users[99];
		const offboardTarget = users[97];
		const ordinaryUser = users[0];
		const existingAdmins = await adminApi.get(
			`/api/v1/admin/organizations/${tenantId}/administrators?page=1&per_page=100`
		);
		expect(existingAdmins.ok()).toBeTruthy();
		baselineAdminIds = ((await existingAdmins.json()).administrators ?? [])
			.filter((admin: { disabled_at: string | null }) => admin.disabled_at === null)
			.map((admin: { user_id: string }) => admin.user_id);
		for (const organizationAdmin of [firstOrganizationAdmin, secondOrganizationAdmin]) {
			const grant = await adminApi.post(
				`/api/v1/admin/organizations/${tenantId}/administrators/${organizationAdmin.id}`,
				{ headers: operator.headers, data: {} }
			);
			expect(grant.status()).toBe(201);
			grantedAdminIds.push(organizationAdmin.id);
		}

		const ordinaryContext = await browser.newContext({
			baseURL: process.env.E2E_BASE_URL ?? 'http://127.0.0.1'
		});
		const ordinaryPage = await ordinaryContext.newPage();
		try {
			const ordinary = await login(ordinaryPage, ordinaryUser.email, ordinaryUser.password);
			const denied = await ordinary.api.get('/api/v1/organization-admin/users?page=1&per_page=50');
			expect(denied.status()).toBe(403);
		} finally {
			await ordinaryContext.close();
		}

		const organizationAdminContext = await browser.newContext({
			baseURL: process.env.E2E_BASE_URL ?? 'http://127.0.0.1'
		});
		const organizationAdminPage = await organizationAdminContext.newPage();
		try {
			await login(
				organizationAdminPage,
				secondOrganizationAdmin.email,
				secondOrganizationAdmin.password
			);
			await organizationAdminPage.goto('/organization-admin/users');
			await expect(organizationAdminPage.getByRole('heading', { name: 'Users' })).toBeVisible();
			const search = organizationAdminPage.getByPlaceholder('Search name or email');
			await search.fill(usernamePrefix);
			await organizationAdminPage.getByRole('button', { name: 'Search', exact: true }).click();
			await expect(organizationAdminPage.getByText('100 users · Page 1 of 2')).toBeVisible();
			const rows = organizationAdminPage.locator('tbody tr');
			await expect(rows).toHaveCount(50);
			const pageOneEmails = await rows.locator('td:nth-child(3)').allTextContents();

			await organizationAdminPage.getByRole('button', { name: 'Next' }).click();
			await expect(organizationAdminPage.getByText('100 users · Page 2 of 2')).toBeVisible();
			await expect(rows).toHaveCount(50);
			const pageTwoEmails = await rows.locator('td:nth-child(3)').allTextContents();
			expect(new Set([...pageOneEmails, ...pageTwoEmails]).size).toBe(100);
			expect(
				[...pageOneEmails, ...pageTwoEmails].every((email) => email.includes(emailPrefix))
			).toBe(true);

			await search.fill(offboardTarget.email);
			await organizationAdminPage.getByRole('button', { name: 'Search', exact: true }).click();
			await expect(organizationAdminPage.getByText('1 users · Page 1 of 1')).toBeVisible();
			await expect(rows).toHaveCount(1);
			await organizationAdminPage
				.getByRole('checkbox', { name: `Select ${offboardTarget.displayName}` })
				.check();

			const previewResponsePromise = organizationAdminPage.waitForResponse(
				(response) =>
					response.url().endsWith('/api/v1/organization-admin/users/offboard') &&
					response.request().method() === 'POST'
			);
			await organizationAdminPage.getByRole('button', { name: 'Preview offboarding' }).click();
			const preview = await (await previewResponsePromise).json();
			expect(preview).toMatchObject({
				dry_run: true,
				rows: [{ user_id: offboardTarget.id, outcome: 'ready' }]
			});
			const dialog = organizationAdminPage.getByRole('dialog', {
				name: 'Confirm user offboarding'
			});
			await expect(dialog).toContainText(offboardTarget.displayName);
			await expect(dialog).toContainText('ready');

			const applyResponsePromise = organizationAdminPage.waitForResponse(
				(response) =>
					response.url().endsWith('/api/v1/organization-admin/users/offboard') &&
					response.request().method() === 'POST'
			);
			await organizationAdminPage.getByRole('button', { name: 'Confirm disable' }).click();
			const applied = await (await applyResponsePromise).json();
			expect(applied).toMatchObject({
				dry_run: false,
				rows: [{ user_id: offboardTarget.id, outcome: 'disabled' }]
			});
			await expect(dialog).toContainText('Applied');

			await organizationAdminPage.goto('/organization-admin/audit');
			await expect(
				organizationAdminPage.getByRole('heading', { name: 'Organization audit' })
			).toBeVisible();
			const auditRow = organizationAdminPage
				.locator('tbody tr')
				.filter({ hasText: 'organization_admin.user_offboarded' });
			await expect(auditRow.first()).toContainText(offboardTarget.id);
			await expect(auditRow.first()).toContainText(secondOrganizationAdmin.id);
		} finally {
			await organizationAdminContext.close();
		}
	} finally {
		const cleanupFailures: string[] = [];
		const retainLastAdmin = baselineAdminIds.length === 0 && grantedAdminIds.length > 0;
		const retainedAdminId = retainLastAdmin ? grantedAdminIds[grantedAdminIds.length - 1] : null;
		for (const adminId of grantedAdminIds) {
			if (adminId === retainedAdminId) continue;
			try {
				const revoked = await adminApi.delete(
					`/api/v1/admin/organizations/${tenantId}/administrators/${adminId}`,
					{ headers: operator.headers }
				);
				if (revoked.status() !== 204) cleanupFailures.push('revoke organization-admin grant');
			} catch {
				cleanupFailures.push('revoke organization-admin grant');
			}
		}
		for (let offset = users.length - 1; offset >= 0; offset -= 1) {
			if (users[offset].id === retainedAdminId) continue;
			try {
				const removed = await adminApi.delete(`/api/v1/admin/users/${users[offset].id}`, {
					headers: operator.headers
				});
				if (!removed.ok()) cleanupFailures.push(`delete ${users[offset].username}`);
			} catch {
				cleanupFailures.push(`delete ${users[offset].username}`);
			}
		}
		if (users.length === 100) {
			try {
				const remaining = await adminApi.get(
					`/api/v1/admin/users?search=${encodeURIComponent(usernamePrefix)}&per_page=100`
				);
				const remainingBody = remaining.ok() ? await remaining.json() : null;
				const expectedRemaining = retainLastAdmin ? 1 : 0;
				if (
					remainingBody?.total !== expectedRemaining ||
					(retainLastAdmin && remainingBody.users?.[0]?.id !== retainedAdminId)
				) {
					cleanupFailures.push('verify synthetic-account cleanup');
				}
			} catch {
				cleanupFailures.push('verify synthetic-account cleanup');
			}
			if (retainLastAdmin) {
				try {
					const remainingAdmins = await adminApi.get(
						`/api/v1/admin/organizations/${tenantId}/administrators?page=1&per_page=100`
					);
					const adminBody = remainingAdmins.ok() ? await remainingAdmins.json() : null;
					const activeAdminIds = (adminBody?.administrators ?? [])
						.filter((admin: { disabled_at: string | null }) => admin.disabled_at === null)
						.map((admin: { user_id: string }) => admin.user_id);
					if (
						activeAdminIds.length !== baselineAdminIds.length + 1 ||
						!activeAdminIds.includes(retainedAdminId!) ||
						!baselineAdminIds.every((adminId) => activeAdminIds.includes(adminId))
					) {
						cleanupFailures.push('verify required last-admin survivor');
					}
				} catch {
					cleanupFailures.push('verify required last-admin survivor');
				}
			}
		}
		expect(cleanupFailures, 'synthetic account teardown').toEqual([]);
	}
});
