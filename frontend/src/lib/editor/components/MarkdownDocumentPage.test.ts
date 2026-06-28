import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import MarkdownDocumentPageTestWrapper from './MarkdownDocumentPage.test.wrapper.svelte';
import { WRITE_PERMISSIONS, READ_ONLY_PERMISSIONS } from '../types';

// Helper to create a Svelte-compatible mock component
function createMockComponent(instanceMethods = {}) {
	return function MockComponent(options: any) {
		const instance = {
			$on: vi.fn(),
			$set: vi.fn(),
			$destroy: vi.fn(),
			...instanceMethods
		};
		// Svelte 4 calls components with 'new'
		// If we return an object from a constructor, it becomes the instance
		return instance;
	};
}

vi.mock('./RichMarkdownEditor.svelte', () => ({
	default: createMockComponent({
		getMarkdown: () => 'updated content',
		getEditor: () => ({ commands: { focus: vi.fn() } })
	})
}));

vi.mock('./RichMarkdownViewer.svelte', () => ({
	default: createMockComponent()
}));

vi.mock('./AttachmentPanel.svelte', () => ({
	default: createMockComponent()
}));

describe('MarkdownDocumentPage', () => {
	const defaultProps = {
		title: 'Test Doc',
		content: '# Initial Content',
		mode: 'read' as const,
		permissions: READ_ONLY_PERMISSIONS,
		saveStatus: 'saved' as const
	};

	beforeEach(() => {
		vi.clearAllMocks();
		vi.useFakeTimers();
	});

	it('renders title and label correctly', () => {
		const { getAllByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			label: 'Documents'
		});

		// Title appears in both main header and hidden print view
		expect(getAllByText('Test Doc').length).toBeGreaterThanOrEqual(1);
		expect(getAllByText('Documents').length).toBeGreaterThanOrEqual(1);
	});

	it('hides edit button for read-only users', () => {
		const { queryByText } = render(MarkdownDocumentPageTestWrapper, defaultProps);
		expect(queryByText('Edit')).toBeNull();
	});

	it('shows edit button when write permission is granted', () => {
		const { getByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS
		});
		expect(getByText('Edit')).toBeTruthy();
	});

	it('renders edit mode with save indicator', async () => {
		const { getByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved'
		});

		// In edit mode, the mode toggle shows "Read" and the save indicator shows "Unsaved"
		expect(getByText('Read')).toBeTruthy();
		expect(getByText('Unsaved')).toBeTruthy();
	});

	it('handles Cmd+S shortcut', async () => {
		render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved'
		});

		await fireEvent.keyDown(window, { key: 's', metaKey: true });
		// Event triggered
	});

	it('shows saving status in edit mode', async () => {
		const { getByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'saving'
		});

		expect(getByText('Saving…')).toBeTruthy();
		expect(getByText('Read')).toBeTruthy();
	});

	it('shows error status when save fails', async () => {
		const { getByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'error'
		});

		expect(getByText('Error')).toBeTruthy();
	});

	it('renders the provided title independently of the first H1', () => {
		const { getAllByText, queryAllByText } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			title: 'Display Title',
			content: '# H1 Title\n\nBody text.'
		});

		expect(getAllByText('Display Title').length).toBeGreaterThanOrEqual(1);
		expect(queryAllByText('H1 Title').length).toBe(0);
	});

	it('preserves frontmatter when dispatching save', async () => {
		const saveHandler = vi.fn();
		render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved',
			content: '---\ntitle: YAML Title\nid: note-123\n---\n# Body H1\n\nBody text.',
			onSave: saveHandler
		});

		await fireEvent.keyDown(window, { key: 's', metaKey: true });

		await waitFor(() => {
			expect(saveHandler).toHaveBeenCalledTimes(1);
		});
		const savedContent = saveHandler.mock.calls[0][0].detail.content;
		expect(savedContent).toContain('title: YAML Title');
		expect(savedContent).toContain('id: note-123');
		expect(savedContent).toContain('updated content');
		expect(savedContent.startsWith('---\n')).toBe(true);
	});

	it('does not dispatch rename when the heading is edited and saved', async () => {
		const saveHandler = vi.fn();
		const renameHandler = vi.fn();
		render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved',
			content: '# Original H1\n\nBody text.',
			onSave: saveHandler,
			onRename: renameHandler
		});

		await fireEvent.keyDown(window, { key: 's', metaKey: true });

		await waitFor(() => {
			expect(saveHandler).toHaveBeenCalledTimes(1);
		});
		expect(renameHandler).not.toHaveBeenCalled();
	});

	it('saves content unchanged for documents without frontmatter', async () => {
		const saveHandler = vi.fn();
		render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved',
			content: '# Decision\n\nBody text.',
			onSave: saveHandler
		});

		await fireEvent.keyDown(window, { key: 's', metaKey: true });

		await waitFor(() => {
			expect(saveHandler).toHaveBeenCalledTimes(1);
		});
		expect(saveHandler.mock.calls[0][0].detail.content).toBe('updated content');
	});

	it('allows inline title edit and dispatches rename on Enter', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		expect(titleEl).not.toBeNull();
		await fireEvent.click(titleEl!);

		const input = container.querySelector('input.doc-title-input') as HTMLInputElement;
		expect(input).not.toBeNull();
		expect(input.value).toBe('Test Doc');

		input.value = 'Renamed Doc';
		await fireEvent.input(input);
		await fireEvent.keyDown(input, { key: 'Enter' });

		await waitFor(() => {
			expect(renameHandler).toHaveBeenCalledTimes(1);
		});
		expect(renameHandler.mock.calls[0][0].detail).toEqual({ title: 'Renamed Doc' });
	});

	it('does not allow inline title edit for read-only users', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: READ_ONLY_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		expect(titleEl).toBeNull();

		const readonlyTitle = container.querySelector('h1.doc-title');
		expect(readonlyTitle).not.toBeNull();
		await fireEvent.click(readonlyTitle!);

		expect(container.querySelector('input.doc-title-input')).toBeNull();
		expect(renameHandler).not.toHaveBeenCalled();
	});

	it('cancels inline title edit on Escape', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		await fireEvent.click(titleEl!);

		const input = container.querySelector('input.doc-title-input') as HTMLInputElement;
		input.value = 'New Title';
		await fireEvent.input(input);
		await fireEvent.keyDown(input, { key: 'Escape' });

		await waitFor(() => {
			expect(renameHandler).not.toHaveBeenCalled();
		});
		expect(container.querySelector('input.doc-title-input')).toBeNull();
		expect(container.querySelector('h1.doc-title, h1.doc-title-wrapper')?.textContent?.trim()).toBe(
			'Test Doc'
		);
	});

	it('dispatches rename on blur when title changed', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		await fireEvent.click(titleEl!);

		const input = container.querySelector('input.doc-title-input') as HTMLInputElement;
		input.value = 'Blurred Title';
		await fireEvent.input(input);
		await fireEvent.blur(input);

		await waitFor(() => {
			expect(renameHandler).toHaveBeenCalledTimes(1);
		});
		expect(renameHandler.mock.calls[0][0].detail).toEqual({ title: 'Blurred Title' });
	});

	it('does not dispatch rename when title is unchanged', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		await fireEvent.click(titleEl!);

		const input = container.querySelector('input.doc-title-input') as HTMLInputElement;
		await fireEvent.blur(input);

		await waitFor(() => {
			expect(renameHandler).not.toHaveBeenCalled();
		});
		expect(container.querySelector('input.doc-title-input')).toBeNull();
	});

	it('cancels inline edit when title is empty', async () => {
		const renameHandler = vi.fn();
		const { container } = render(MarkdownDocumentPageTestWrapper, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS,
			onRename: renameHandler
		});

		const titleEl = container.querySelector('button.doc-title-button');
		await fireEvent.click(titleEl!);

		const input = container.querySelector('input.doc-title-input') as HTMLInputElement;
		input.value = '   ';
		await fireEvent.input(input);
		await fireEvent.keyDown(input, { key: 'Enter' });

		await waitFor(() => {
			expect(renameHandler).not.toHaveBeenCalled();
		});
		expect(container.querySelector('input.doc-title-input')).toBeNull();
		expect(container.querySelector('h1.doc-title-wrapper')?.textContent?.trim()).toBe('Test Doc');
	});
});
