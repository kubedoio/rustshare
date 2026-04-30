import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import NotesModuleView from './NotesModuleView.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	createFromTemplate: vi.fn(),
	getModuleSummary: vi.fn()
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api/modules', () => ({
	createFromTemplate: mocks.createFromTemplate,
	getModuleSummary: mocks.getModuleSummary
}));

describe('NotesModuleView', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		mocks.createFromTemplate.mockResolvedValue({
			object_id: 'note-123',
			object_type: 'file'
		});
		mocks.getModuleSummary.mockResolvedValue({
			recent_items: [
				{
					id: 'note-1',
					name: 'Architecture.md',
					updated_at: '2026-04-30T10:00:00Z'
				}
			]
		});
	});

	it('opens the dedicated note route after creating a note', async () => {
		render(NotesModuleView, {
			moduleConfig: {
				module_key: 'notes',
				display_name: 'Notes',
				description: 'Capture notes.',
				icon: 'sticky-note',
				default_template: 'template_default_note'
			}
		});

		await fireEvent.click(screen.getByRole('button', { name: 'New Note' }));

		await waitFor(() => {
			expect(mocks.goto).toHaveBeenCalledWith('/notes/note-123');
		});
	});

	it('links recent notes to the note editor route', async () => {
		render(NotesModuleView, {
			moduleConfig: {
				module_key: 'notes',
				display_name: 'Notes',
				description: 'Capture notes.',
				icon: 'sticky-note',
				default_template: 'template_default_note'
			}
		});

		const link = await screen.findByRole('link', { name: /Architecture\.md/i });
		expect(link.getAttribute('href')).toBe('/notes/note-1');
	});
});
