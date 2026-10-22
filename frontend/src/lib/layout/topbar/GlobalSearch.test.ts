import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import GlobalSearch from './GlobalSearch.svelte';

describe('GlobalSearch historical meetings', () => {
	it('shows a generic search error without stale result details', () => {
		render(GlobalSearch, {
			props: {
				value: 'private phrase',
				results: { files: [], folders: [], meetings: [] },
				loading: false,
				error: true,
				onChange: vi.fn(),
				onClear: vi.fn(),
				onSelect: vi.fn()
			}
		});

		expect(screen.getByText('Search is temporarily unavailable.')).toBeTruthy();
		expect(screen.queryByText(/No results found/)).toBeNull();
	});

	it('shows an authorized meeting result and opens its meeting context', async () => {
		const onSelect = vi.fn();
		render(GlobalSearch, {
			props: {
				value: 'budget',
				results: {
					files: [],
					folders: [],
					meetings: [
						{
							id: 'meeting-note-1',
							name: 'Budget review',
							path: '',
							snippet: 'Decision: retain funding'
						}
					]
				},
				onChange: vi.fn(),
				onClear: vi.fn(),
				onSelect
			}
		});

		expect(screen.getByText('Decision: retain funding')).toBeTruthy();
		await fireEvent.click(screen.getByText('Budget review').closest('button')!);
		expect(onSelect).toHaveBeenCalledWith('meeting', 'meeting-note-1');
	});
});
