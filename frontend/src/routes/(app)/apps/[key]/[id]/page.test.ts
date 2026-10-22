import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { readable, writable } from 'svelte/store';
import Page from './+page.svelte';

const { markdownDocumentPageMock, mockPageStore, mockMutateAsync } = vi.hoisted(() => {
	type PageValue = { params: { key: string; id: string }; url: URL };
	let value: PageValue = {
		params: { key: 'notes', id: 'note-1' },
		url: new URL('http://localhost/apps/notes/note-1')
	};
	const subscribers = new Set<(value: PageValue) => void>();

	return {
		markdownDocumentPageMock: vi.fn((..._args: unknown[]) => null),
		mockMutateAsync: vi.fn(),
		mockPageStore: {
			subscribe(run: (value: PageValue) => void) {
				subscribers.add(run);
				run(value);
				return () => subscribers.delete(run);
			},
			set(next: PageValue) {
				value = next;
				for (const run of subscribers) run(value);
			}
		}
	};
});

vi.mock('$app/environment', () => ({ browser: true }));

vi.mock('$app/stores', () => ({
	page: mockPageStore
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

type MockQuery = ReturnType<typeof import('$lib/query-compat').createQuery> & {
	setData: (data: unknown) => void;
};

function createMockQuery(data: unknown): MockQuery {
	const store = writable<{
		data: unknown;
		isLoading: boolean;
		isFetching: boolean;
		error: null;
		refetch: ReturnType<typeof vi.fn>;
	}>({
		data: { routeKey: 'notes', routeId: 'note-1', item: data },
		isLoading: false,
		isFetching: true,
		error: null,
		refetch: vi.fn()
	});
	return {
		subscribe: store.subscribe,
		setData: (nextData: unknown) => store.update((value) => ({ ...value, data: nextData })),
		setOptions: vi.fn()
	} as unknown as MockQuery;
}

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn(() => createMockQuery(null)),
	createMutation: vi.fn(() =>
		readable({
			mutate: vi.fn(),
			mutateAsync: mockMutateAsync,
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

vi.mock('$lib/applications/registry', () => ({
	applicationsStore: readable([
		{
			key: 'notes',
			displayName: 'Notes',
			rootPath: '/Notes'
		}
	])
}));

vi.mock('$lib/stores/toast', () => ({
	toastStore: {
		show: vi.fn()
	}
}));

vi.mock('$lib/editor/components/MarkdownDocumentPage.svelte', () => ({
	default: markdownDocumentPageMock
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

beforeEach(() => {
	mockMutateAsync.mockResolvedValue({});
	mockPageStore.set({
		params: { key: 'notes', id: 'note-1' },
		url: new URL('http://localhost/apps/notes/note-1')
	});
});

describe('Note detail page conflict banner', () => {
	it('updates the query and attachment state when the application route changes', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const query = createMockQuery({ id: 'note-1', name: 'Note', content: 'Body', metadata: {} });
		vi.mocked(createQuery).mockReturnValue(query);

		render(Page);
		await waitFor(() => {
			expect(query.setOptions).toHaveBeenLastCalledWith(
				expect.objectContaining({ queryKey: ['module-item', 'notes', 'note-1'] })
			);
		});

		const callsBeforeRouteChange = markdownDocumentPageMock.mock.calls.length;
		mockPageStore.set({
			params: { key: 'meetings', id: 'note-1' },
			url: new URL('http://localhost/apps/meetings/note-1?attachments=open')
		});

		await waitFor(() => {
			expect(query.setOptions).toHaveBeenLastCalledWith(
				expect.objectContaining({ queryKey: ['module-item', 'meetings', 'note-1'] })
			);
		});
		expect(markdownDocumentPageMock).toHaveBeenCalledTimes(callsBeforeRouteChange);
		query.setData({
			routeKey: 'meetings',
			routeId: 'note-1',
			item: { id: 'note-1', name: 'Meeting', content: 'Body', metadata: {} }
		});
		await waitFor(() =>
			expect(markdownDocumentPageMock).toHaveBeenCalledTimes(callsBeforeRouteChange + 1)
		);
		const props = (markdownDocumentPageMock.mock.calls.at(-1)?.[1] ??
			markdownDocumentPageMock.mock.calls.at(-1)?.[0]) as unknown as {
			initialAttachmentsOpen: boolean;
		};
		expect(props.initialAttachmentsOpen).toBe(true);
	});

	it('keeps a pending save attached to the route where it started', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const query = createMockQuery({
			id: 'note-1',
			name: 'Note',
			content: 'Old body',
			metadata: {}
		});
		vi.mocked(createQuery).mockReturnValue(query);
		let resolveSave: ((value: unknown) => void) | undefined;
		mockMutateAsync.mockImplementation(() => new Promise((resolve) => (resolveSave = resolve)));

		render(Page);
		const editorCall = markdownDocumentPageMock.mock.calls.at(-1) ?? [];
		const editorProps = (editorCall[1] ?? editorCall[0]) as unknown as {
			$$events: { save: (event: CustomEvent<{ content: string }>) => Promise<void> };
		};
		const save = editorProps.$$events.save(
			new CustomEvent('save', { detail: { content: 'Saved old body' } })
		);

		mockPageStore.set({
			params: { key: 'meetings', id: 'note-1' },
			url: new URL('http://localhost/apps/meetings/note-1')
		});
		await waitFor(() =>
			expect(query.setOptions).toHaveBeenLastCalledWith(
				expect.objectContaining({ queryKey: ['module-item', 'meetings', 'note-1'] })
			)
		);
		const newEditorProps = (markdownDocumentPageMock.mock.calls.at(-1)?.[1] ?? {}) as unknown as {
			saveStatus: string;
		};
		expect(newEditorProps.saveStatus).toBe('saved');
		resolveSave?.({ modified_at: '2026-01-01T00:00:00Z' });
		await save;

		const { queryClient } = await import('$lib/query-client');
		expect(queryClient.setQueryData).toHaveBeenCalledWith(
			['module-item', 'notes', 'note-1'],
			expect.any(Function)
		);
		expect(queryClient.setQueryData).not.toHaveBeenCalledWith(
			['module-item', 'meetings', 'note-1'],
			expect.any(Function)
		);
	});

	it('passes the saved note name separately from its Markdown H1', async () => {
		const { createQuery } = await import('$lib/query-compat');
		vi.mocked(createQuery).mockReturnValue(
			createMockQuery({
				id: 'note-1',
				name: 'note.md',
				content: '---\ntitle: Frontmatter title\n---\n# Independent Markdown H1\n\nBody.',
				metadata: {
					title: 'Renamed note name',
					visibility: 'private',
					public_share_id: null,
					created_at: new Date().toISOString(),
					updated_at: new Date().toISOString(),
					excerpt: ''
				},
				parent_folder_id: null,
				current_version: 1,
				created_at: new Date().toISOString(),
				modified_at: new Date().toISOString()
			})
		);

		render(Page);

		const call = markdownDocumentPageMock.mock.calls.at(-1) as unknown as unknown[];
		const props = (call[1] ?? call[0]) as {
			title: string;
			subtitle: string;
			content: string;
		};
		expect(props.title).toBe('Renamed note name');
		expect(props.subtitle).toBe('Independent Markdown H1');
		expect(props.content).toContain('# Independent Markdown H1');
	});

	it('does not render a cached note whose id belongs to the previous route', async () => {
		const { createQuery } = await import('$lib/query-compat');
		vi.mocked(createQuery).mockReturnValue(
			createMockQuery({
				id: 'note-2',
				name: 'Previous note',
				content: 'Previous note content',
				metadata: {}
			})
		);

		render(Page);

		expect(screen.queryByText('Previous note')).toBeNull();
	});

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
