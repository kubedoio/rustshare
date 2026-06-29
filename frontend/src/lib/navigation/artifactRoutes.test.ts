import { describe, it, expect, vi } from 'vitest';
import { navigateToNote } from './artifactRoutes';

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

describe('navigateToNote', () => {
	it('navigates to the notes module detail page without a return url', async () => {
		const { goto } = await import('$app/navigation');
		navigateToNote('note-1');
		expect(goto).toHaveBeenCalledWith('/modules/notes/note-1');
	});

	it('includes a returnTo query parameter when a return url is provided', async () => {
		const { goto } = await import('$app/navigation');
		navigateToNote('note-1', '/files?folder=abc-123');
		expect(goto).toHaveBeenCalledWith('/modules/notes/note-1?returnTo=%2Ffiles%3Ffolder%3Dabc-123');
	});
});
