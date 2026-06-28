import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import Page from './+page.svelte';

vi.mock('$app/environment', () => ({ browser: true }));

vi.mock('$app/stores', () => ({
	page: readable({
		params: { key: 'notes', id: 'note-1' },
		url: new URL('http://localhost/modules/notes/note-1')
	})
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn(),
	beforeNavigate: vi.fn(() => vi.fn())
}));

vi.mock('$lib/query-client', () => ({
	queryClient: {
		invalidateQueries: vi.fn(),
		setQueryData: vi.fn()
	}
}));

function createMockQuery(data: unknown) {
	const store = readable({
		data,
		isLoading: false,
		refetch: vi.fn()
	});
	return {
		...store,
		setOptions: vi.fn()
	} as unknown as ReturnType<typeof import('$lib/query-compat').createQuery>;
}

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() => createMockQuery(null)),
	createMutation: vi.fn(() =>
		readable({
			mutate: vi.fn(),
			mutateAsync: vi.fn(),
			isPending: false
		})
	)
}));

vi.mock('$lib/api/notes', () => ({
	notesApi: {
		get: vi.fn(),
		list: vi.fn(),
		create: vi.fn(),
		update: vi.fn(),
		delete: vi.fn(),
		toggleVisibility: vi.fn()
	},
	renameNote: vi.fn(),
	moveNote: vi.fn(),
	deleteNote: vi.fn(),
	duplicateNote: vi.fn(),
	resolveConflict: vi.fn()
}));

vi.mock('$lib/api/files', () => ({
	uploadFile: vi.fn(),
	deleteFile: vi.fn()
}));

vi.mock('$lib/api/folders', () => ({
	getFolderContents: vi.fn()
}));

vi.mock('$lib/modules/registry', () => ({
	getModuleByKey: vi.fn(() => ({
		key: 'notes',
		displayName: 'Notes',
		rootPath: '/Notes'
	}))
}));

vi.mock('$lib/stores/toast', () => ({
	toastStore: {
		show: vi.fn()
	}
}));

vi.mock('$lib/editor/components/MarkdownDocumentPage.svelte', () => ({
	default: vi.fn(() => null)
}));

vi.mock('$lib/components/modals/ShareModal.svelte', () => ({
	default: vi.fn(() => null)
}));

vi.mock('$lib/components/modals/MoveModal.svelte', () => ({
	default: vi.fn(() => null)
}));

vi.mock('$lib/components/modals/DeleteConfirmation.svelte', () => ({
	default: vi.fn(() => null)
}));

vi.mock('$lib/components/common/PromptModal.svelte', () => ({
	default: vi.fn(() => null)
}));

vi.mock('lucide-svelte', () => ({
	Folder: vi.fn(() => null),
	Share2: vi.fn(() => null),
	Pencil: vi.fn(() => null),
	AlertTriangle: vi.fn(() => null),
	X: vi.fn(() => null)
}));

describe('Note detail page conflict banner', () => {
	it('renders resolution actions for title_mismatch conflicts', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const mockedCreateQuery = vi.mocked(createQuery);
		mockedCreateQuery.mockReturnValue(
			createMockQuery({
				id: 'note-1',
				name: 'note.md',
				content: '',
				metadata: {
					title: 'Folder Title',
					visibility: 'private',
					public_share_id: null,
					created_at: new Date().toISOString(),
					updated_at: new Date().toISOString(),
					excerpt: '',
					mime_type: 'text/markdown',
					extension: 'md',
					conflict: {
						kind: 'title_mismatch',
						message: 'Title mismatch',
						yaml_title: 'YAML Title',
						folder_name: 'Folder Title'
					}
				},
				parent_folder_id: null,
				current_version: 1,
				created_at: new Date().toISOString(),
				modified_at: new Date().toISOString()
			})
		);

		render(Page);

		expect(screen.getByText('Conflict: title_mismatch')).toBeTruthy();
		expect(screen.getByRole('button', { name: /Use YAML title/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /Use folder name/i })).toBeTruthy();
		expect(screen.getByRole('button', { name: /Custom title/i })).toBeTruthy();
	});

	it('renders resolution actions for identity_mismatch conflicts', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const mockedCreateQuery = vi.mocked(createQuery);
		mockedCreateQuery.mockReturnValue(
			createMockQuery({
				id: 'note-1',
				name: 'note.md',
				content: '',
				metadata: {
					title: 'Note',
					visibility: 'private',
					public_share_id: null,
					created_at: new Date().toISOString(),
					updated_at: new Date().toISOString(),
					excerpt: '',
					mime_type: 'text/markdown',
					extension: 'md',
					conflict: {
						kind: 'identity_mismatch',
						message: 'Identity mismatch',
						yaml_id: 'yaml-id',
						sidecar_id: 'sidecar-id'
					}
				},
				parent_folder_id: null,
				current_version: 1,
				created_at: new Date().toISOString(),
				modified_at: new Date().toISOString()
			})
		);

		render(Page);

		expect(screen.getByText('Conflict: identity_mismatch')).toBeTruthy();
		expect(screen.getByText('Identity conflict: manual file edit required.')).toBeTruthy();
		expect(screen.queryByRole('button', { name: /Use YAML ID/i })).toBeNull();
		expect(screen.queryByRole('button', { name: /Use sidecar ID/i })).toBeNull();
		expect(screen.getByRole('button', { name: /Dismiss conflict warning/i })).toBeTruthy();
	});
});
