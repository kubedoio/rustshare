import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import FilePreview from './FilePreview.svelte';
import type { Folder } from '$lib/api/types';

describe('FilePreview', () => {
	it('shows note bundle indicator for folders with note_bundle_file_id', () => {
		const { container } = render(FilePreview, {
			props: {
				item: {
					id: '1',
					name: 'My Note',
					path: '/My Note',
					parent_folder_id: null,
					owner_id: 'user-1',
					created_at: '2024-01-01T00:00:00Z',
					updated_at: '2024-01-01T00:00:00Z',
					note_bundle_file_id: 'abc-123'
				} as Folder,
				isFolder: true
			}
		});
		expect(container.querySelector('.text-brand-400')).toBeTruthy();
	});

	it('shows muted settings icon for _rustshare folders', () => {
		const { container } = render(FilePreview, {
			props: {
				item: {
					id: '1',
					name: '_rustshare',
					path: '/_rustshare',
					parent_folder_id: null,
					owner_id: 'user-1',
					created_at: '2024-01-01T00:00:00Z',
					updated_at: '2024-01-01T00:00:00Z'
				} as Folder,
				isFolder: true
			}
		});
		expect(container.querySelector('.text-base-content\\/40')).toBeTruthy();
	});
});
