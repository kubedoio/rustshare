/**
 * Admin panel end-to-end tests.
 *
 * Prerequisites: the full Docker stack must be running (`docker compose up`).
 * Default target: http://localhost (nginx → backend).
 * Override with: E2E_BASE_URL=http://my-host npx playwright test
 */

import { expect, test, type Page } from '@playwright/test';

const BASE = process.env.E2E_BASE_URL ?? 'http://localhost';

// Seed credentials from docker-compose defaults
const ADMIN_EMAIL = 'admin@localhost';
const ADMIN_PASSWORD = 'admin123';

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

	// Open create modal
	await page.click('button:has-text("Create User")');
	await page.waitForSelector('input#username', { timeout: 5_000 });

	await page.fill('input#username', username);
	await page.fill('input#email', email);
	await page.fill('input#password', 'TestPass123!');

	await page.click('button:has-text("Create")');

	// Modal should close and user should appear in list
	await expect(page.locator(`text=${username}`)).toBeVisible({ timeout: 8_000 });
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
	await page.click('button:has-text("Create User")');
	await page.waitForSelector('input#username');

	await page.fill('input#username', username);
	await page.fill('input#email', email);
	await page.fill('input#password', password);
	await page.click('button:has-text("Create")');
	await expect(page.locator(`text=${username}`)).toBeVisible({ timeout: 8_000 });

	// Navigate to the user detail page and disable
	await page.click(`a[href*="/admin/users/"]:near(:text("${username}"))`);
	await page.waitForURL(/\/admin\/users\/.+/);
	await page.click('button:has-text("Disable")');
	// Confirm if a dialog appears
	const confirmBtn = page.locator('button:has-text("Confirm")');
	if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
		await confirmBtn.click();
	}
	await expect(page.locator('text=disabled', { exact: false })).toBeVisible({ timeout: 5_000 });

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

	// Re-enable the user
	await page.click('button:has-text("Enable")');
	await expect(page.locator('text=active', { exact: false })).toBeVisible({ timeout: 5_000 });

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
	await page.click('button:has-text("Create User")');
	await page.waitForSelector('input#username');
	await page.fill('input#username', memberUsername);
	await page.fill('input#email', memberEmail);
	await page.fill('input#password', 'TestPass123!');
	await page.click('button:has-text("Create")');
	await expect(page.locator(`text=${memberUsername}`)).toBeVisible({ timeout: 8_000 });

	// Create group
	await page.goto(`${BASE}/admin/groups`);
	await page.click('button:has-text("Create Group")');
	await page.waitForSelector('input#name');
	await page.fill('input#name', groupName);
	await page.click('button:has-text("Create")');
	await expect(page.locator(`text=${groupName}`)).toBeVisible({ timeout: 8_000 });

	// Open group detail
	await page.click(`a:has-text("${groupName}")`);
	await page.waitForURL(/\/admin\/groups\/.+/);

	// Add member via typeahead
	const searchInput = page.getByPlaceholder('Search users...');
	await searchInput.fill(memberUsername.slice(0, 6));
	await expect(page.locator(`text=${memberUsername}`).first()).toBeVisible({ timeout: 5_000 });
	await page.locator(`text=${memberUsername}`).first().click();

	// Member should now appear in the member list
	await expect(page.locator(`td:has-text("${memberUsername}")`)).toBeVisible({ timeout: 5_000 });

	// Remove the member
	await page.click(`tr:has-text("${memberUsername}") button:has-text("Remove")`);
	await expect(page.locator(`td:has-text("${memberUsername}")`)).not.toBeVisible({ timeout: 5_000 });

	// Delete the group
	await page.goto(`${BASE}/admin/groups`);
	await page.click(`tr:has-text("${groupName}") button:has-text("Delete")`);
	const confirmBtn = page.locator('button:has-text("Confirm")');
	if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
		await confirmBtn.click();
	}
	await expect(page.locator(`text=${groupName}`)).not.toBeVisible({ timeout: 5_000 });
});

// ---------------------------------------------------------------------------
// Test 4: OIDC config persists across page reload
// ---------------------------------------------------------------------------

test('OIDC config values persist after save and page reload', async ({ page }) => {
	await loginAsAdmin(page);
	await page.goto(`${BASE}/admin/oidc`);
	await page.waitForSelector('#issuer-url', { timeout: 8_000 });

	const issuerUrl = 'https://accounts.e2e-test.example.com';
	const providerId = `e2e-client-${uniqueSlug()}`;

	// Fill in the form
	await page.fill('#issuer-url', issuerUrl);
	await page.fill('#client-id', providerId);
	await page.fill('#provider-name', 'E2E Test Provider');

	// Save
	await page.click('button:has-text("Save")');
	await expect(page.locator('text=saved', { exact: false })).toBeVisible({ timeout: 5_000 });

	// Reload and verify values are still there
	await page.reload();
	await page.waitForSelector('#issuer-url', { timeout: 8_000 });
	await expect(page.locator('#issuer-url')).toHaveValue(issuerUrl);
	await expect(page.locator('#client-id')).toHaveValue(providerId);

	// Clean up: disable OIDC (restore to default state)
	const enableToggle = page.locator('input[type="checkbox"].toggle');
	if (await enableToggle.isChecked()) {
		await enableToggle.uncheck();
		await page.click('button:has-text("Save")');
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

	// Create a user
	await page.goto(`${BASE}/admin/users`);
	await page.click('button:has-text("Create User")');
	await page.waitForSelector('input#username');
	await page.fill('input#username', username);
	await page.fill('input#email', email);
	await page.fill('input#password', 'TestPass123!');
	await page.click('button:has-text("Create")');
	await expect(page.locator(`text=${username}`)).toBeVisible({ timeout: 8_000 });

	// Navigate to user detail and disable
	await page.click(`a[href*="/admin/users/"]:near(:text("${username}"))`);
	await page.waitForURL(/\/admin\/users\/.+/);
	await page.click('button:has-text("Disable")');
	const confirmBtn = page.locator('button:has-text("Confirm")');
	if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
		await confirmBtn.click();
	}
	await expect(page.locator('text=disabled', { exact: false })).toBeVisible({ timeout: 5_000 });

	// Check audit log for the disable action
	await page.goto(`${BASE}/admin/audit`);
	await page.waitForSelector('table', { timeout: 8_000 });

	// Filter by admin_action type to narrow results
	const typeSelect = page.locator('select').first();
	if (await typeSelect.isVisible()) {
		await typeSelect.selectOption('admin_action');
	}

	await expect(
		page.locator('text=user.disabled', { exact: false }).first()
	).toBeVisible({ timeout: 8_000 });
});
