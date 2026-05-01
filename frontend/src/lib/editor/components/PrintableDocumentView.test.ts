import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import PrintableDocumentView from './PrintableDocumentView.svelte';
import { formatExportFilename } from '../adapter/export';

describe('PrintableDocumentView', () => {
	const props = {
		title: 'Print Test',
		content: '# Hello World\n\nThis is a test.',
		label: 'Documents',
		date: '2026-05-01'
	};

	it('renders title, label and date', () => {
		const { getByText } = render(PrintableDocumentView, props);

		expect(getByText('Print Test')).toBeTruthy();
		expect(getByText('Documents')).toBeTruthy();
		expect(getByText('2026-05-01')).toBeTruthy();
	});

	it('renders markdown content', () => {
		const { getByText } = render(PrintableDocumentView, props);
		expect(getByText('Hello World')).toBeTruthy();
		expect(getByText('This is a test.')).toBeTruthy();
	});

	it('hides header when showHeader is false', () => {
		const { queryByText } = render(PrintableDocumentView, {
			...props,
			showHeader: false
		});

		expect(queryByText('Print Test')).toBeNull();
	});
});

describe('Export Utilities', () => {
	it('formats export filename correctly', () => {
		expect(formatExportFilename('My Document', 'md')).toBe('my-document.md');
		expect(formatExportFilename('Project: Plan 2026!', 'pdf')).toBe('project-plan-2026.pdf');
		expect(formatExportFilename('   Spaces   ', 'md')).toBe('spaces.md');
		expect(formatExportFilename('', 'md')).toBe('document.md');
	});
});
