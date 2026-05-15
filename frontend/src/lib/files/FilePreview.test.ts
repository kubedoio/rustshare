import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import FilePreview from './FilePreview.svelte';

describe('FilePreview', () => {
	it('shows note bundle indicator for folders with note_bundle_file_id', () => {
		const { container } = render(FilePreview, {
			props: {
				item: { id: '1', name: 'My Note', note_bundle_file_id: 'abc-123' },
				isFolder: true
			}
		});
		expect(container.querySelector('.text-brand-400')).toBeTruthy();
	});

	it('shows muted settings icon for _rustshare folders', () => {
		const { container } = render(FilePreview, {
			props: {
				item: { id: '1', name: '_rustshare' },
				isFolder: true
			}
		});
		expect(container.querySelector('.text-base-content\\/40')).toBeTruthy();
	});
});
