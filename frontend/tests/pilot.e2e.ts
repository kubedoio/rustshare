import { expect, test } from '@playwright/test';

const adminEmail = process.env.ADMIN_EMAIL ?? '';
const adminPassword = process.env.ADMIN_PASSWORD ?? '';
const pilotFileName = process.env.PILOT_FILE_NAME ?? '';
const pilotNoteId = process.env.PILOT_NOTE_ID ?? '';
const pilotNoteTitle = process.env.PILOT_NOTE_TITLE ?? '';

if (!adminEmail || !adminPassword || !pilotFileName || !pilotNoteId || !pilotNoteTitle) {
	throw new Error(
		'ADMIN_EMAIL, ADMIN_PASSWORD, PILOT_FILE_NAME, PILOT_NOTE_ID, and PILOT_NOTE_TITLE are required for the pilot browser test'
	);
}

test('pilot administrator uses Files and edits a Note name independently from its H1', async ({
	page
}) => {
	test.setTimeout(90_000);
	page.setDefaultTimeout(15_000);
	page.setDefaultNavigationTimeout(20_000);

	await test.step('Authenticate and open Files', async () => {
		await page.goto('/login');
		await page.getByLabel('Email').fill(adminEmail);
		await page.getByLabel('Password').fill(adminPassword);
		await page.getByRole('button', { name: 'Sign in with password' }).click();

		await page.waitForURL('**/files', { timeout: 10_000 });
		await expect(page.getByRole('heading', { name: 'My Files' })).toBeVisible();
	});

	await test.step('Open pilot folder and verify its File', async () => {
		const smokeFolderButtons = page
			.locator('tbody tr')
			.getByRole('button', { name: 'Beta Smoke', exact: true });
		await expect(smokeFolderButtons.first()).toBeVisible({ timeout: 10_000 });
		const folderCount = await smokeFolderButtons.count();
		let pilotFolderOpened = false;

		for (let index = 0; index < folderCount; index += 1) {
			if (index > 0) {
				await page.goto('/files');
				await expect(smokeFolderButtons.nth(index)).toBeVisible({ timeout: 10_000 });
			}

			await smokeFolderButtons.nth(index).click();
			await expect(page.getByRole('heading', { name: 'Beta Smoke' })).toBeVisible();
			try {
				await expect(page.getByText(pilotFileName, { exact: true })).toBeVisible({
					timeout: 2_000
				});
				pilotFolderOpened = true;
				break;
			} catch {
				// Repeated canonical smoke runs can leave folders with the same display name.
			}
		}

		expect(pilotFolderOpened).toBe(true);
	});

	let noteOpened = false;
	await test.step('Open pilot Note', async () => {
		await page.goto(`/apps/notes/${encodeURIComponent(pilotNoteId)}`);
		await expect(page.locator('h1.doc-title-wrapper')).toBeVisible({ timeout: 10_000 });
		noteOpened = true;
	});
	const noteName = page.locator('h1.doc-title-wrapper');
	const markdownH1 = page.locator('.doc-subtitle');
	const originalH1 = 'Beta Smoke H1';
	const editedH1 = 'Beta Smoke H1 edited in browser';
	const renamedNote = `${pilotNoteTitle} renamed in browser`;

	const editH1 = async (nextH1: string) => {
		const editorH1 = page.locator('.ProseMirror h1');
		await expect(editorH1).toBeVisible();
		await editorH1.click();
		await page.keyboard.press('Home');
		await page.keyboard.press('Shift+End');
		await page.keyboard.type(nextH1);
		const saved = page.waitForResponse(
			(response) =>
				response.url().includes(`/api/v1/notes/${pilotNoteId}`) &&
				response.request().method() === 'PUT'
		);
		await page.getByRole('button', { name: 'Read', exact: true }).click();
		expect((await saved).ok()).toBeTruthy();
		await expect(markdownH1).toHaveText(nextH1);
	};

	const rename = async (currentName: string, nextName: string) => {
		await page.getByRole('button', { name: `${currentName}, edit title` }).click();
		const input = page.getByRole('textbox', { name: 'Edit document title' });
		await input.fill(nextName);
		const saved = page.waitForResponse(
			(response) =>
				response.url().endsWith(`/api/v1/notes/${pilotNoteId}/rename`) &&
				response.request().method() === 'POST'
		);
		await input.press('Enter');
		expect((await saved).ok()).toBeTruthy();
		await expect(noteName).toHaveText(nextName);
	};

	const restoreSmokeNote = async () => {
		await page.reload();
		await expect(noteName).toBeVisible({ timeout: 10_000 });
		const currentH1 = (await markdownH1.textContent())?.trim() ?? '';
		const currentName = (await noteName.textContent())?.trim() ?? '';
		if (currentH1 !== originalH1) await editH1(originalH1);
		if (currentName !== pilotNoteTitle) await rename(currentName, pilotNoteTitle);
		await page.reload();
		await expect(noteName).toHaveText(pilotNoteTitle, { timeout: 10_000 });
		await expect(markdownH1).toHaveText(originalH1);
	};

	try {
		await test.step('Verify original Note name and Markdown H1', async () => {
			await expect(noteName).toHaveText(pilotNoteTitle, { timeout: 10_000 });
			await expect(markdownH1).toHaveText(originalH1);
		});

		await test.step('Edit Markdown H1 without renaming Note', async () => {
			await editH1(editedH1);
			await expect(noteName).toHaveText(pilotNoteTitle);
		});

		await test.step('Rename Note without changing Markdown H1', async () => {
			await rename(pilotNoteTitle, renamedNote);
			await expect(markdownH1).toHaveText(editedH1);
		});

		await test.step('Reload and verify both values persist', async () => {
			await page.reload();
			await expect(noteName).toHaveText(renamedNote, { timeout: 10_000 });
			await expect(markdownH1).toHaveText(editedH1);
		});
	} finally {
		if (noteOpened) await test.step('Restore pilot Note fixture', restoreSmokeNote);
	}
});
