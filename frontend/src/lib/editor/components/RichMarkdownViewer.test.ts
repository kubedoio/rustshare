// Regression tests for RichMarkdownViewer link handling:
// - external links must open with `noopener,noreferrer` (reverse tabnabbing);
// - attachments with an empty path/filename must not hijack link resolution
//   (previously `url.includes('')` matched every URL).

import { render, fireEvent } from '@testing-library/svelte';
import { describe, it, expect, vi, afterEach } from 'vitest';
import RichMarkdownViewer from './RichMarkdownViewer.svelte';
import type { RichMarkdownAttachment } from '../types';

function makeAttachment(overrides: Partial<RichMarkdownAttachment>): RichMarkdownAttachment {
	return {
		id: 'att-1',
		filename: 'diagram.png',
		path: './attachments/diagram.png',
		mimeType: 'image/png',
		size: 100,
		kind: 'image',
		createdAt: '2026-01-01T00:00:00Z',
		createdBy: 'user-1',
		...overrides
	};
}

afterEach(() => {
	vi.restoreAllMocks();
});

describe('RichMarkdownViewer external links', () => {
	it('opens external links with noopener,noreferrer', async () => {
		const open = vi.spyOn(window, 'open').mockImplementation(() => null);
		const { container } = render(RichMarkdownViewer, {
			props: { content: '[Example](https://example.com)' }
		});

		const anchor = container.querySelector('a') as HTMLAnchorElement | null;
		expect(anchor).not.toBeNull();
		expect(anchor!.getAttribute('href')).toBe('https://example.com');

		await fireEvent.click(anchor!);
		expect(open).toHaveBeenCalledWith('https://example.com', '_blank', 'noopener,noreferrer');
	});

	it('falls back to external open instead of resolving to an empty-path attachment', async () => {
		const open = vi.spyOn(window, 'open').mockImplementation(() => null);
		const emptyPathAttachment = makeAttachment({ id: 'att-1', path: '', filename: '' });
		const { container } = render(RichMarkdownViewer, {
			props: {
				content: '[External](https://attacker.example)',
				attachments: [emptyPathAttachment]
			}
		});

		const anchor = container.querySelector('a') as HTMLAnchorElement | null;
		await fireEvent.click(anchor!);

		// The empty-path attachment must NOT capture the link; it is treated as
		// external and opened in a new tab with noopener,noreferrer.
		expect(open).toHaveBeenCalledWith('https://attacker.example', '_blank', 'noopener,noreferrer');
	});

	it('still resolves links that genuinely match an attachment by id (no external open)', async () => {
		const open = vi.spyOn(window, 'open').mockImplementation(() => null);
		const attachment = makeAttachment({ id: 'att-1' });
		const { container } = render(RichMarkdownViewer, {
			props: {
				content: '[diagram](/api/v1/files/att-1/preview)',
				attachments: [attachment]
			}
		});

		const anchor = container.querySelector('a') as HTMLAnchorElement | null;
		await fireEvent.click(anchor!);

		// A genuine attachment match consumes the click (dispatches the open
		// event internally); it must not fall through to window.open.
		expect(open).not.toHaveBeenCalled();
	});
});
