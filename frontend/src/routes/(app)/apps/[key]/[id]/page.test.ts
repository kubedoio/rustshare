import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { readable, writable } from 'svelte/store';
import Page from './+page.svelte';

const { markdownDocumentPageMock, mockEditorMethods, mockPageStore, mockMutateAsync } = vi.hoisted(
	() => {
		type PageValue = { params: { key: string; id: string }; url: URL };
		let value: PageValue = {
			params: { key: 'notes', id: 'note-1' },
			url: new URL('http://localhost/apps/notes/note-1')
		};
		const subscribers = new Set<(value: PageValue) => void>();
		const mockEditorMethods = {
			flush: vi.fn(),
			markSaved: vi.fn(),
			markSaveError: vi.fn(),
			insertAttachment: vi.fn()
		};

		return {
			markdownDocumentPageMock: vi.fn((..._args: unknown[]) => mockEditorMethods),
			mockEditorMethods,
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
	}
);

function editorProps() {
	const call = markdownDocumentPageMock.mock.calls.at(-1) ?? [];
	return (call[1] ?? call[0]) as {
		saveStatus: string;
		attachments: { id: string }[];
		$$events: {
			save: (event: CustomEvent<{ content: string }>) => Promise<void>;
			upload: (event: CustomEvent<{ files: File[] }>) => void;
			sketch: (event: CustomEvent<{ blob: Blob; filename: string }>) => void;
		};
	};
}

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: Error) => void;
	const promise = new Promise<T>((resolvePromise, rejectPromise) => {
		resolve = resolvePromise;
		reject = rejectPromise;
	});
	return { promise, resolve, reject };
}

function noteAttachment(fileId: string) {
	return {
		file_id: fileId,
		name: `${fileId}.png`,
		mime_type: 'image/png',
		size: 10,
		created_at: '2026-01-01T00:00:00Z'
	};
}

function noteItem(id: string, fileIds: string[] = []) {
	return {
		id,
		name: 'note.md',
		content: 'Body',
		parent_folder_id: `folder-${id}`,
		metadata: { attachments: fileIds.map(noteAttachment) }
	};
}

function navigateToNote(query: MockQuery, id: string, fileIds: string[] = []) {
	mockPageStore.set({
		params: { key: 'notes', id },
		url: new URL(`http://localhost/apps/notes/${id}`)
	});
	query.setData({ routeKey: 'notes', routeId: id, item: noteItem(id, fileIds) });
}

vi.mock('$app/environment', () => ({ browser: true }));

vi.mock('$app/stores', () => ({
	page: mockPageStore
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn(),
	beforeNavigate: vi.fn(() => vi.fn())
}));

vi.mock('$lib/stores/auth', () => ({ currentUser: readable(null) }));

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
	vi.clearAllMocks();
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
		const initialProps = (markdownDocumentPageMock.mock.calls.at(-1)?.[1] ??
			markdownDocumentPageMock.mock.calls.at(-1)?.[0]) as unknown as { mode: string };
		expect(initialProps.mode).toBe('edit');

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
			mode: string;
		};
		expect(props.initialAttachmentsOpen).toBe(true);
		expect(props.mode).toBe('read');
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
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith({
			queryKey: ['module-item', 'notes', 'note-1'],
			exact: true,
			refetchType: 'none'
		});
		expect(queryClient.setQueryData).not.toHaveBeenCalled();
	});

	it('does not carry attachments into a cached destination note', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const query = createMockQuery(noteItem('note-1', ['source-file']));
		vi.mocked(createQuery).mockReturnValue(query);
		render(Page);
		await waitFor(() =>
			expect(editorProps().attachments.map((a) => a.id)).toEqual(['source-file'])
		);

		navigateToNote(query, 'note-2', ['destination-file']);
		await waitFor(() =>
			expect(editorProps().attachments.map((a) => a.id)).toEqual(['destination-file'])
		);
		await editorProps().$$events.save(new CustomEvent('save', { detail: { content: 'Saved' } }));
		expect(mockMutateAsync).toHaveBeenLastCalledWith(
			expect.objectContaining({ noteAttachments: [noteAttachment('destination-file')] })
		);
		const { queryClient } = await import('$lib/query-client');
		expect(queryClient.setQueryData).toHaveBeenCalledWith(
			['module-item', 'notes', 'note-2'],
			expect.any(Function)
		);
	});

	it('marks the source stale rather than patching it when a save finishes after unmount', async () => {
		const { createQuery } = await import('$lib/query-compat');
		vi.mocked(createQuery).mockReturnValue(createMockQuery(noteItem('note-1')));
		const oldSave = deferred<unknown>();
		mockMutateAsync.mockImplementationOnce(() => oldSave.promise);
		const { unmount } = render(Page);
		const saving = editorProps().$$events.save(
			new CustomEvent('save', { detail: { content: 'Old edit' } })
		);
		unmount();
		oldSave.resolve({});
		await saving;

		const { queryClient } = await import('$lib/query-client');
		expect(queryClient.setQueryData).not.toHaveBeenCalled();
		expect(queryClient.invalidateQueries).toHaveBeenCalledWith(
			expect.objectContaining({ queryKey: ['module-item', 'notes', 'note-1'], refetchType: 'none' })
		);
		expect(mockEditorMethods.markSaved).not.toHaveBeenCalled();
	});

	it.each(['success', 'error'] as const)(
		'does not apply an old save %s after leaving and returning to the same note',
		async (outcome) => {
			const { createQuery } = await import('$lib/query-compat');
			const query = createMockQuery(noteItem('note-1'));
			vi.mocked(createQuery).mockReturnValue(query);
			const oldSave = deferred<unknown>();
			const newSave = deferred<unknown>();
			mockMutateAsync
				.mockImplementationOnce(() => oldSave.promise)
				.mockImplementationOnce(() => newSave.promise);
			render(Page);
			const savingOld = editorProps().$$events.save(
				new CustomEvent('save', { detail: { content: 'Old edit' } })
			);

			navigateToNote(query, 'note-2');
			await waitFor(() =>
				expect(query.setOptions).toHaveBeenLastCalledWith(
					expect.objectContaining({ queryKey: ['module-item', 'notes', 'note-2'] })
				)
			);
			navigateToNote(query, 'note-1');
			await waitFor(() =>
				expect(query.setOptions).toHaveBeenLastCalledWith(
					expect.objectContaining({ queryKey: ['module-item', 'notes', 'note-1'] })
				)
			);
			const savingNew = editorProps().$$events.save(
				new CustomEvent('save', { detail: { content: 'New edit' } })
			);
			await waitFor(() => expect(editorProps().saveStatus).toBe('saving'));

			if (outcome === 'success') oldSave.resolve({});
			else oldSave.reject(new Error('Old request failed'));
			await savingOld;
			expect(editorProps().saveStatus).toBe('saving');
			expect(mockEditorMethods.markSaved).not.toHaveBeenCalled();
			expect(mockEditorMethods.markSaveError).not.toHaveBeenCalled();
			const { queryClient } = await import('$lib/query-client');
			expect(queryClient.setQueryData).not.toHaveBeenCalled();

			newSave.resolve({});
			await savingNew;
			expect(editorProps().saveStatus).toBe('saved');
			expect(mockEditorMethods.markSaved).toHaveBeenCalledWith('New edit');
		}
	);

	it.each(['upload', 'sketch'] as const)(
		'does not attach or delete a completed stale %s after navigation',
		async (kind) => {
			const { createQuery } = await import('$lib/query-compat');
			const { uploadFile, deleteFile } = await import('$lib/api/files');
			const { getFolderContents } = await import('$lib/api/folders');
			const query = createMockQuery(noteItem('note-1'));
			vi.mocked(createQuery).mockReturnValue(query);
			vi.mocked(getFolderContents).mockResolvedValue({
				folders: [
					{ id: 'attachment-folder', name: kind === 'upload' ? 'attachments' : 'drawings' }
				],
				files: []
			} as never);
			const upload = deferred<Awaited<ReturnType<typeof uploadFile>>>();
			vi.mocked(uploadFile).mockImplementationOnce(() => upload.promise);
			render(Page);
			const file = new File(['image'], 'image.png', { type: 'image/png' });
			if (kind === 'upload')
				editorProps().$$events.upload(new CustomEvent('upload', { detail: { files: [file] } }));
			else
				editorProps().$$events.sketch(
					new CustomEvent('sketch', { detail: { blob: file, filename: file.name } })
				);
			await waitFor(() => expect(uploadFile).toHaveBeenCalledOnce());

			navigateToNote(query, 'note-2', ['destination-file']);
			await waitFor(() =>
				expect(editorProps().attachments.map((a) => a.id)).toEqual(['destination-file'])
			);
			upload.resolve({
				id: 'uploaded-file',
				name: file.name,
				mime_type: file.type,
				size: file.size,
				created_at: '2026-01-01T00:00:00Z'
			} as Awaited<ReturnType<typeof uploadFile>>);
			await upload.promise;
			await new Promise((resolve) => setTimeout(resolve, 0));
			expect(editorProps().attachments.map((a) => a.id)).toEqual(['destination-file']);
			expect(mockEditorMethods.insertAttachment).not.toHaveBeenCalled();
			expect(deleteFile).not.toHaveBeenCalled();
		}
	);

	it('preserves an unsaved upload when the same note is refetched', async () => {
		const { createQuery } = await import('$lib/query-compat');
		const { uploadFile } = await import('$lib/api/files');
		const { getFolderContents } = await import('$lib/api/folders');
		const query = createMockQuery(noteItem('note-1', ['server-file']));
		vi.mocked(createQuery).mockReturnValue(query);
		vi.mocked(getFolderContents).mockResolvedValue({ folders: [], files: [] } as never);
		vi.mocked(uploadFile).mockResolvedValueOnce({
			id: 'local-file',
			name: 'local.png',
			mime_type: 'image/png',
			size: 10,
			created_at: '2026-01-01T00:00:00Z'
		} as Awaited<ReturnType<typeof uploadFile>>);
		render(Page);
		editorProps().$$events.upload(
			new CustomEvent('upload', {
				detail: { files: [new File(['image'], 'local.png', { type: 'image/png' })] }
			})
		);
		await waitFor(() =>
			expect(editorProps().attachments.map((a) => a.id)).toEqual(['server-file', 'local-file'])
		);

		query.setData({
			routeKey: 'notes',
			routeId: 'note-1',
			item: noteItem('note-1', ['server-file'])
		});
		await new Promise((resolve) => setTimeout(resolve, 0));
		expect(editorProps().attachments.map((a) => a.id)).toEqual(['server-file', 'local-file']);
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
