import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import FileToolbar from './FileToolbar.svelte';

describe('FileToolbar', () => {
	it('gives the New folder and Upload buttons accessible names', () => {
		render(FileToolbar);

		// The visible text labels hide below the sm breakpoint, so the buttons
		// must keep an accessible name (aria-label) at all widths.
		const newFolder = screen.getByRole('button', { name: 'New folder' });
		const upload = screen.getByRole('button', { name: 'Upload' });

		expect(newFolder.getAttribute('aria-label')).toBe('New folder');
		expect(newFolder.getAttribute('title')).toBe('New folder');
		expect(upload.getAttribute('aria-label')).toBe('Upload');
		expect(upload.getAttribute('title')).toBe('Upload');
	});

	it('omits the New folder and Upload buttons when not permitted', () => {
		render(FileToolbar, { props: { canCreateFolder: false, canUpload: false } });

		expect(screen.queryByRole('button', { name: 'New folder' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Upload' })).toBeNull();
	});
});
