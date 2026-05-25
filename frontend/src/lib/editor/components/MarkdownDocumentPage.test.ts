import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import MarkdownDocumentPage from './MarkdownDocumentPage.svelte';
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
		const { getAllByText } = render(MarkdownDocumentPage, {
			...defaultProps,
			label: 'Documents'
		});

		// Title appears in both main header and hidden print view
		expect(getAllByText('Test Doc').length).toBeGreaterThanOrEqual(1);
		expect(getAllByText('Documents').length).toBeGreaterThanOrEqual(1);
	});

	it('hides edit button for read-only users', () => {
		const { queryByText } = render(MarkdownDocumentPage, defaultProps);
		expect(queryByText('Edit')).toBeNull();
	});

	it('shows edit button when write permission is granted', () => {
		const { getByText } = render(MarkdownDocumentPage, {
			...defaultProps,
			permissions: WRITE_PERMISSIONS
		});
		expect(getByText('Edit')).toBeTruthy();
	});

	it('renders edit mode with save indicator', async () => {
		const { getByText } = render(MarkdownDocumentPage, {
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
		render(MarkdownDocumentPage, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'unsaved'
		});

		await fireEvent.keyDown(window, { key: 's', metaKey: true });
		// Event triggered
	});

	it('shows saving status in edit mode', async () => {
		const { getByText } = render(MarkdownDocumentPage, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'saving'
		});

		expect(getByText('Saving…')).toBeTruthy();
		expect(getByText('Read')).toBeTruthy();
	});

	it('shows error status when save fails', async () => {
		const { getByText } = render(MarkdownDocumentPage, {
			...defaultProps,
			mode: 'edit',
			permissions: WRITE_PERMISSIONS,
			saveStatus: 'error'
		});

		expect(getByText('Error')).toBeTruthy();
	});
});
