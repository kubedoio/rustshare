import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import NotesModuleView from './NotesModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listNotes: vi.fn(),
	createNote: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/notes', () => ({
	listNotes: mocks.listNotes,
	createNote: mocks.createNote
}));

describe('NotesModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		queryClient.clear();
		mocks.createNote.mockResolvedValue({
			id: 'note-123',
			name: 'Untitled Note',
			path: '/Notes/Untitled Note.md',
			content: '# Untitled Note\n\n',
			metadata: {},
			parent_folder_id: null,
			current_version: 1,
			created_at: '2026-04-30T10:00:00Z',
			modified_at: '2026-04-30T10:00:00Z',
			public_url: null
		});
		mocks.listNotes.mockResolvedValue([
			{
				id: 'note-1',
				name: 'Architecture.md',
				modified_at: '2026-04-30T10:00:00Z'
			}
		]);
	});

	it('opens the dedicated note route after creating a note', async () => {
		render(NotesModuleView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				schemaVersion: '1.0',
				permissions: {
					adminCanConfigure: true,
					workspaceMembersCanUse: true,
					allowPublicShare: false,
					allowInternalShare: true
				},
				ui: {
					sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Notes',
							description: 'Recent notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/modules/notes',
						renderer: 'notes',
						layout: 'list-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first note.',
						primaryAction: { label: 'New note', action: 'create-from-template' }
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		await fireEvent.click(screen.getByRole('button', { name: 'New note' }));

		await waitFor(() => {
			expect(mocks.goto).toHaveBeenCalledWith('/modules/notes/note-123');
		});
	});

	it('links recent notes to the note editor route', async () => {
		render(NotesModuleView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				schemaVersion: '1.0',
				permissions: {
					adminCanConfigure: true,
					workspaceMembersCanUse: true,
					allowPublicShare: false,
					allowInternalShare: true
				},
				ui: {
					sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Notes',
							description: 'Recent notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/modules/notes',
						renderer: 'notes',
						layout: 'list-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first note.',
						primaryAction: { label: 'New note', action: 'create-from-template' }
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		const link = await screen.findByRole('link', { name: /Architecture/i });
		expect(link.getAttribute('href')).toBe('/modules/notes/note-1');
	});

	it('renders attachment and drawing counts in grid view', async () => {
		mocks.listNotes.mockResolvedValue([
			{
				id: 'note-1',
				name: 'Note with Assets.md',
				modified_at: '2026-04-30T10:00:00Z',
				metadata: { title: 'Note with Assets', excerpt: 'Has attachments and drawings.' },
				attachment_count: 3,
				drawing_count: 1
			}
		]);

		render(NotesModuleView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				schemaVersion: '1.0',
				permissions: {
					adminCanConfigure: true,
					workspaceMembersCanUse: true,
					allowPublicShare: false,
					allowInternalShare: true
				},
				ui: {
					sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Notes',
							description: 'Recent notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/modules/notes',
						renderer: 'notes',
						layout: 'gallery-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first note.',
						primaryAction: { label: 'New note', action: 'create-from-template' }
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		await screen.findByText('Note with Assets');
		expect(screen.getAllByText('3').length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText('1').length).toBeGreaterThanOrEqual(1);
	});

	it('renders attachment and drawing counts in list view', async () => {
		mocks.listNotes.mockResolvedValue([
			{
				id: 'note-1',
				name: 'Note with Assets.md',
				modified_at: '2026-04-30T10:00:00Z',
				metadata: { title: 'Note with Assets', excerpt: 'Has attachments and drawings.' },
				attachment_count: 3,
				drawing_count: 1
			}
		]);

		render(NotesModuleView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				schemaVersion: '1.0',
				permissions: {
					adminCanConfigure: true,
					workspaceMembersCanUse: true,
					allowPublicShare: false,
					allowInternalShare: true
				},
				ui: {
					sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Notes',
							description: 'Recent notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/modules/notes',
						renderer: 'notes',
						layout: 'list-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first note.',
						primaryAction: { label: 'New note', action: 'create-from-template' }
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		await screen.findByText('Note with Assets');
		expect(screen.getAllByText('3').length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText('1').length).toBeGreaterThanOrEqual(1);
	});

	it('does not render badges when counts are zero or undefined', async () => {
		mocks.listNotes.mockResolvedValue([
			{
				id: 'note-1',
				name: 'Plain Note.md',
				modified_at: '2026-04-30T10:00:00Z',
				metadata: { title: 'Plain Note', excerpt: 'Nothing special.' },
				attachment_count: 0,
				drawing_count: undefined
			}
		]);

		render(NotesModuleView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				schemaVersion: '1.0',
				permissions: {
					adminCanConfigure: true,
					workspaceMembersCanUse: true,
					allowPublicShare: false,
					allowInternalShare: true
				},
				ui: {
					sidebar: { enabled: true, order: 10, icon: 'sticky-note', label: 'Notes' },
					dashboard: {
						enabled: true,
						order: 10,
						widget: {
							enabled: true,
							type: 'notes-recent',
							title: 'Notes',
							description: 'Recent notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/modules/notes',
						renderer: 'notes',
						layout: 'list-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first note.',
						primaryAction: { label: 'New note', action: 'create-from-template' }
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		await screen.findByText('Plain Note');
		expect(screen.queryByText('0')).toBeFalsy();
		expect(screen.queryByText('undefined')).toBeFalsy();
	});
});
