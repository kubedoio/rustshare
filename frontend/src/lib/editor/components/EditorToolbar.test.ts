import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import EditorToolbar from './EditorToolbar.svelte';

function createMockEditor() {
	return {
		isActive: vi.fn(() => false),
		can: vi.fn(() => ({ chain: () => ({ run: vi.fn() }) })),
		chain: vi.fn(() => ({
			focus: vi.fn(() => ({ run: vi.fn() })),
			toggleHeading: vi.fn(() => ({ run: vi.fn() })),
			toggleBold: vi.fn(() => ({ run: vi.fn() })),
			toggleItalic: vi.fn(() => ({ run: vi.fn() })),
			toggleUnderline: vi.fn(() => ({ run: vi.fn() })),
			toggleBulletList: vi.fn(() => ({ run: vi.fn() })),
			toggleOrderedList: vi.fn(() => ({ run: vi.fn() })),
			toggleTaskList: vi.fn(() => ({ run: vi.fn() })),
			toggleBlockquote: vi.fn(() => ({ run: vi.fn() })),
			toggleCodeBlock: vi.fn(() => ({ run: vi.fn() })),
			toggleCode: vi.fn(() => ({ run: vi.fn() })),
			unsetLink: vi.fn(() => ({ run: vi.fn() })),
			setLink: vi.fn(() => ({ run: vi.fn() })),
			insertTable: vi.fn(() => ({ run: vi.fn() })),
			undo: vi.fn(() => ({ run: vi.fn() })),
			redo: vi.fn(() => ({ run: vi.fn() })),
			splitCell: vi.fn(() => ({ run: vi.fn() })),
			mergeCells: vi.fn(() => ({ run: vi.fn() })),
			toggleHeaderRow: vi.fn(() => ({ run: vi.fn() })),
			addColumnAfter: vi.fn(() => ({ run: vi.fn() })),
			deleteColumn: vi.fn(() => ({ run: vi.fn() })),
			addRowAfter: vi.fn(() => ({ run: vi.fn() })),
			deleteRow: vi.fn(() => ({ run: vi.fn() })),
			deleteTable: vi.fn(() => ({ run: vi.fn() })),
			setHorizontalRule: vi.fn(() => ({ run: vi.fn() }))
		})),
		on: vi.fn(),
		off: vi.fn()
	} as unknown as import('@tiptap/core').Editor;
}

describe('EditorToolbar', () => {
	it('renders a sticky toolbar that stays visible while scrolling', () => {
		const { container } = render(EditorToolbar, {
			props: { editor: createMockEditor(), hasAttachmentHandler: false }
		});
		const toolbar = container.querySelector('.editor-toolbar');
		expect(toolbar).not.toBeNull();
		expect(toolbar?.classList.contains('editor-toolbar--sticky')).toBe(true);
	});
});
