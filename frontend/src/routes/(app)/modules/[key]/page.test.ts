import { render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/types';

let moduleResponse: unknown;
let moduleError: Error | null = null;

vi.mock('$app/stores', () => ({
	page: readable({
		params: { key: 'notes' },
		url: new URL('http://localhost/modules/notes')
	})
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() =>
		readable({
			data: moduleResponse,
			error: moduleError,
			isLoading: false
		})
	)
}));

vi.mock('$lib/api/modules', () => ({
	getModule: vi.fn()
}));

vi.mock('$lib/components/modules/NotesModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/KanbanModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/MeetingsModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/StandupsModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/DecisionsModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/SharesModuleView.svelte', () => ({
	default: vi.fn()
}));
vi.mock('$lib/components/modules/GenericModuleView.svelte', () => ({
	default: vi.fn()
}));

import ModulePage from './+page.svelte';

describe('module route page', () => {
	beforeEach(() => {
		moduleResponse = null;
		moduleError = null;
		vi.clearAllMocks();
	});

	it('shows a dedicated not-found state for unknown modules', async () => {
		moduleError = new ApiError(404, 'Module not found');

		render(ModulePage);

		await waitFor(() => {
			expect(screen.getByText('Module Not Found')).toBeTruthy();
		});
	});

	it('shows the backend access message for disabled or forbidden modules', async () => {
		moduleError = new ApiError(403, 'Module disabled');

		render(ModulePage);

		await waitFor(() => {
			expect(screen.getByText('Module Not Available')).toBeTruthy();
		});
		expect(screen.getByText('Module disabled')).toBeTruthy();
	});
});
