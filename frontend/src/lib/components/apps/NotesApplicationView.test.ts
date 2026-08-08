import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { queryClient } from '$lib/query-client';
import NotesApplicationView from './NotesApplicationView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	listNotes: vi.fn(),
	createNote: vi.fn(),
	renameNote: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/notes', () => ({
	listNotes: mocks.listNotes,
	createNote: mocks.createNote,
	renameNote: mocks.renameNote
}));

describe('NotesApplicationView', () => {
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
		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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
			expect(mocks.goto).toHaveBeenCalledWith('/apps/notes/note-123');
		});
	});

	it('links recent notes to the note editor route', async () => {
		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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
		expect(link.getAttribute('href')).toBe('/apps/notes/note-1');
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

		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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

		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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

		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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

	it('supports the OKF-native notes module definition', async () => {
		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Write OKF-compatible notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'okf-note',
				documentFormat: 'okf-markdown',
				defaultTemplate: 'template_default_okf_note',
				icon: 'sticky-note',
				schemaVersion: '1.0',
				okf: {
					enabled: true,
					conceptType: 'Note',
					frontmatterRequired: true,
					preserveUnknownFields: true
				},
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
							type: 'latest-notes',
							title: 'Notes',
							description: 'Recent OKF notes.',
							size: 'medium',
							columns: { desktop: 6, tablet: 12, mobile: 12 },
							maxItems: 4
						}
					},
					page: {
						enabled: true,
						route: '/apps/notes',
						renderer: 'okf-note',
						layout: 'list-grid',
						emptyStateTitle: 'No notes yet',
						emptyStateDescription: 'Create your first OKF note.',
						primaryAction: {
							label: 'New note',
							action: 'create-from-template',
							template: 'template_default_okf_note'
						}
					}
				},
				aiIndexing: { enabled: true },
				audit: { enabled: true }
			}
		});

		const button = screen.getByRole('button', { name: 'New note' });
		expect(button.hasAttribute('disabled')).toBe(false);

		await fireEvent.click(button);

		await waitFor(() => {
			expect(mocks.goto).toHaveBeenCalledWith('/apps/notes/note-123');
		});
	});

	it('rename action changes the display title and keeps the attachments panel working', async () => {
		mocks.renameNote.mockResolvedValue({
			id: 'note-1',
			name: 'New Title.md',
			metadata: { title: 'New Title' }
		});
		mocks.listNotes
			.mockResolvedValueOnce([
				{
					id: 'note-1',
					name: 'Old Title.md',
					modified_at: '2026-04-30T10:00:00Z',
					metadata: { title: 'Old Title', excerpt: 'Old excerpt' },
					attachment_count: 2
				}
			])
			.mockResolvedValueOnce([
				{
					id: 'note-1',
					name: 'New Title.md',
					modified_at: '2026-04-30T10:05:00Z',
					metadata: { title: 'New Title', excerpt: 'Old excerpt' },
					attachment_count: 2
				}
			]);

		render(NotesApplicationView, {
			module: {
				id: 'module_notes',
				key: 'notes',
				displayName: 'Notes',
				description: 'Capture notes.',
				enabled: true,
				rootPath: '/Workspace/Notes',
				renderer: 'notes',
				defaultTemplate: 'template_default_note',
				icon: 'sticky-note',
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
						route: '/apps/notes',
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

		await screen.findByText('Old Title');
		expect(screen.getAllByText('2').length).toBeGreaterThanOrEqual(1);

		await fireEvent.click(screen.getByRole('button', { name: 'More options' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Rename note' }));

		const dialog = screen.getByRole('dialog', { name: 'Rename note' });
		const input = within(dialog).getByRole('textbox');
		await fireEvent.input(input, { target: { value: 'New Title' } });
		await fireEvent.click(within(dialog).getByRole('button', { name: 'Rename' }));

		await waitFor(() => {
			expect(mocks.renameNote).toHaveBeenCalledWith('note-1', { title: 'New Title' });
		});
		await waitFor(() => {
			expect(screen.getByText('New Title')).toBeTruthy();
		});
		expect(screen.getAllByText('2').length).toBeGreaterThanOrEqual(1);
	});
});
