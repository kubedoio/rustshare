/**
 * Admin panel end-to-end tests.
 *
 * Prerequisites: the full Docker stack must be running (`docker compose up`).
 * Default target: http://localhost (nginx → backend).
 * Override with: E2E_BASE_URL=http://my-host npx playwright test
 */

import { expect, test, type Page } from '@playwright/test';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost';

// Seed credentials from environment (must be set before running tests)
const ADMIN_EMAIL = process.env.ADMIN_EMAIL || '';
const ADMIN_PASSWORD = process.env.ADMIN_PASSWORD || '';
if (!ADMIN_EMAIL) {
	throw new Error('ADMIN_EMAIL environment variable is required for e2e tests');
}
if (!ADMIN_PASSWORD) {
	throw new Error('ADMIN_PASSWORD environment variable is required for e2e tests');
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function loginAsAdmin(page: Page) {
	await page.goto(`${BASE}/login`);
	await page.fill('input[type="email"]', ADMIN_EMAIL);
	await page.fill('input[type="password"]', ADMIN_PASSWORD);
	await page.click('button[type="submit"]');
	await page.waitForURL(`${BASE}/files`, { timeout: 10_000 });
}

function uniqueSlug() {
	return `e2e-${Date.now()}`;
}

/** Create a user via the admin users page. Assumes page is already on /admin/users. */
async function createUser(page: Page, username: string, email: string, password: string) {
	await page.click('button:has-text("New User")');
	await page.waitForSelector('input#username', { timeout: 5_000 });
	await page.fill('input#username', username);
	await page.fill('input#email', email);
	await page.fill('input#password', password);
	await page.click('button:has-text("Create User")');
	await expect(page.locator(`td:has-text("${username}")`).first()).toBeVisible({ timeout: 8_000 });
}

// ---------------------------------------------------------------------------
// Test 1: create user → verify in list
// ---------------------------------------------------------------------------

test('admin creates a user and it appears in the user list', async ({ page }) => {
	await loginAsAdmin(page);
	await page.goto(`${BASE}/admin/users`);
	await page.waitForSelector('text=Users', { timeout: 8_000 });

	const slug = uniqueSlug();
	const username = `testuser-${slug}`;
	const email = `${slug}@example.com`;

	await createUser(page, username, email, 'TestPass123!');
	await expect(page.locator(`text=${email}`)).toBeVisible();
});

// ---------------------------------------------------------------------------
// Test 2: disable user → login blocked → re-enable → login succeeds
// ---------------------------------------------------------------------------

test('disabling a user blocks their login; re-enabling restores access', async ({ page, browser }) => {
	await loginAsAdmin(page);

	const slug = uniqueSlug();
	const username = `disabletest-${slug}`;
	const email = `${slug}-disable@example.com`;
	const password = 'TestPass123!';

	// Create the user via the admin panel
	await page.goto(`${BASE}/admin/users`);
	await createUser(page, username, email, password);

	// Disable from the user list row (no confirmation modal — direct action)
	await page.locator(`tr:has-text("${username}") button:has-text("Disable")`).click();
	await expect(page.locator(`tr:has-text("${username}")`).getByText('Disabled')).toBeVisible({ timeout: 5_000 });

	// Try logging in as the disabled user in a new context
	const ctx = await browser.newContext();
	const userPage = await ctx.newPage();
	await userPage.goto(`${BASE}/login`);
	await userPage.fill('input[type="email"]', email);
	await userPage.fill('input[type="password"]', password);
	await userPage.click('button[type="submit"]');
	// Should stay on login page with an error, not redirect to /files
	await expect(userPage).not.toHaveURL(`${BASE}/files`, { timeout: 5_000 });
	await ctx.close();

	// Re-enable the user from the list
	await page.locator(`tr:has-text("${username}") button:has-text("Enable")`).click();
	await expect(page.locator(`tr:has-text("${username}")`).getByText('Active')).toBeVisible({ timeout: 5_000 });

	// Login should now succeed
	const ctx2 = await browser.newContext();
	const userPage2 = await ctx2.newPage();
	await userPage2.goto(`${BASE}/login`);
	await userPage2.fill('input[type="email"]', email);
	await userPage2.fill('input[type="password"]', password);
	await userPage2.click('button[type="submit"]');
	await expect(userPage2).toHaveURL(`${BASE}/files`, { timeout: 10_000 });
	await ctx2.close();
});

// ---------------------------------------------------------------------------
// Test 3: create group → add member via typeahead → remove member → delete
// ---------------------------------------------------------------------------

test('group management: create, add member via typeahead, remove, delete', async ({ page }) => {
	await loginAsAdmin(page);

	const slug = uniqueSlug();
	const groupName = `group-${slug}`;

	// Ensure there is at least one non-admin user to add (create one)
	const memberEmail = `member-${slug}@example.com`;
	const memberUsername = `member-${slug}`;

	await page.goto(`${BASE}/admin/users`);
	await createUser(page, memberUsername, memberEmail, 'TestPass123!');

	// Create group
	await page.goto(`${BASE}/admin/groups`);
	await page.click('button:has-text("New Group")');
	await page.waitForSelector('input#group-name');
	await page.fill('input#group-name', groupName);
	await page.click('button:has-text("Create Group")');
	await expect(page.locator(`text=${groupName}`)).toBeVisible({ timeout: 8_000 });

	// Open group detail
	await page.click(`a:has-text("${groupName}")`);
	await page.waitForURL(/\/admin\/groups\/.+/);

	// Add member via typeahead
	const searchInput = page.getByPlaceholder('Search users to add...');
	await searchInput.fill(memberUsername.slice(0, 6));
	await expect(page.locator(`text=${memberUsername}`).first()).toBeVisible({ timeout: 5_000 });
	await page.locator(`text=${memberUsername}`).first().click();

	// Member should now appear in the member list
	await expect(page.locator(`td:has-text("${memberUsername}")`).first()).toBeVisible({ timeout: 5_000 });

	// Remove the member (opens confirmation modal, then confirm)
	await page.click(`tr:has-text("${memberUsername}") button:has-text("Remove")`);
	await page.locator('.modal.modal-open button.btn-error').click();
	await expect(page.locator(`td:has-text("${memberUsername}")`).first()).not.toBeVisible({ timeout: 5_000 });

	// Delete the group
	await page.goto(`${BASE}/admin/groups`);
	await page.click(`tr:has-text("${groupName}") button:has-text("Delete")`);
	// Confirm in the modal (btn-error = red "Delete" button)
	await page.locator('.modal.modal-open button.btn-error').click();
	await expect(page.locator(`text=${groupName}`)).not.toBeVisible({ timeout: 5_000 });
});

// ---------------------------------------------------------------------------
// Test 4: OIDC config persists across page reload
// ---------------------------------------------------------------------------

test('OIDC config values persist after save and page reload', async ({ page }) => {
	await loginAsAdmin(page);
	await page.goto(`${BASE}/admin/oidc`);
	await page.waitForSelector('input[type="checkbox"].toggle', { timeout: 8_000 });

	const issuerUrl = 'https://accounts.e2e-test.example.com';
	const providerId = `e2e-client-${uniqueSlug()}`;

	// Enable OIDC so the fields become editable
	const enableToggle = page.locator('input[type="checkbox"].toggle');
	if (!(await enableToggle.isChecked())) {
		await enableToggle.check();
	}

	// Fill in the form
	await page.fill('#issuer-url', issuerUrl);
	await page.fill('#client-id', providerId);
	await page.fill('#provider-name', 'E2E Test Provider');

	// Save
	await page.click('button:has-text("Save Configuration")');
	await expect(page.locator('text=Configuration saved')).toBeVisible({ timeout: 5_000 });

	// Reload and verify values are still there
	await page.reload();
	await page.waitForSelector('#issuer-url', { timeout: 8_000 });
	await expect(page.locator('#issuer-url')).toHaveValue(issuerUrl);
	await expect(page.locator('#client-id')).toHaveValue(providerId);

	// Clean up: disable OIDC (restore to default state)
	if (await enableToggle.isChecked()) {
		await enableToggle.uncheck();
		await page.click('button:has-text("Save Configuration")');
	}
});

// ---------------------------------------------------------------------------
// Test 5: disable action appears in audit log
// ---------------------------------------------------------------------------

test('user.disabled action is recorded in the audit log', async ({ page }) => {
	await loginAsAdmin(page);

	const slug = uniqueSlug();
	const username = `audituser-${slug}`;
	const email = `${slug}-audit@example.com`;

	// Create a user and disable them from the list
	await page.goto(`${BASE}/admin/users`);
	await createUser(page, username, email, 'TestPass123!');
	await page.locator(`tr:has-text("${username}") button:has-text("Disable")`).click();
	await expect(page.locator(`tr:has-text("${username}")`).getByText('Disabled')).toBeVisible({ timeout: 5_000 });

	// Check audit log for the disable action
	await page.goto(`${BASE}/admin/audit`);
	await page.waitForSelector('table', { timeout: 8_000 });

	// Filter by admin_action type to narrow results
	await page.locator('#audit-type').selectOption('admin_action');

	await expect(
		page.getByText('user.disabled', { exact: false }).first()
	).toBeVisible({ timeout: 8_000 });
});
