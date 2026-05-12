import { render, screen } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { describe, expect, it, vi } from 'vitest';
import BrainstormBoardPage from './+page.svelte';

vi.mock('$app/environment', () => ({ browser: true }));

vi.mock('$app/stores', () => ({
	page: readable({ params: { boardId: 'test-board-123' } })
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn(),
	beforeNavigate: vi.fn(() => vi.fn())
}));

vi.mock('$lib/query-client', () => ({
	queryClient: {
		invalidateQueries: vi.fn()
	}
}));

vi.mock('$lib/query-compat', () => ({
	createQuery: vi.fn((options: { queryKey?: unknown[] }) => {
		const key = options.queryKey?.[0];
		if (key === 'brainstorm-board') {
			return readable({
				data: { id: 'test-board-123', title: 'Test Board' },
				isLoading: false
			});
		}
		if (key === 'brainstorm-board-source') {
			return readable({
				data: '{"type":"excalidraw","version":2,"elements":[]}',
				isLoading: false
			});
		}
		return readable({ data: null, isLoading: false });
	}),
	createMutation: vi.fn(() => readable({
		mutate: vi.fn(),
		isPending: false
	}))
}));

describe('Brainstorm Board Page', () => {
	it('renders board title', async () => {
		render(BrainstormBoardPage);
		const matches = screen.getAllByText('Test Board');
		expect(matches.length).toBeGreaterThan(0);
	});

	it('shows save status', async () => {
		render(BrainstormBoardPage);
		expect(screen.getByText('Saved')).toBeTruthy();
	});

	it('shows a share button in the header', async () => {
		render(BrainstormBoardPage);
		const shareButton = screen.getByRole('button', { name: /Share/i });
		expect(shareButton).toBeTruthy();
	});
});
