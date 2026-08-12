import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { askHref, askWorkspace, openAskCitation, type AskScope } from './ask';

vi.mock('./client', () => ({
	apiClient: { post: vi.fn() }
}));

describe('Ask API contract', () => {
	beforeEach(() => vi.mocked(apiClient.post).mockReset());

	it.each([
		[{ type: 'workspace' } satisfies AskScope],
		[{ type: 'folder', resourceRef: 'elembra://io.elembra.files/folder/f-1' } satisfies AskScope],
		[{ type: 'note', resourceRef: 'elembra://io.elembra.files/file/n-1' } satisfies AskScope],
		[{ type: 'chatChannel', communityId: 'community-1', channelId: 'channel-1' } satisfies AskScope]
	])('sends the exact server scope %o', async (scope) => {
		vi.mocked(apiClient.post).mockResolvedValue({});
		await askWorkspace({ question: 'Where?', workspace_id: 'tenant-1', scope });
		expect(apiClient.post).toHaveBeenCalledWith('/memory/ask', {
			question: 'Where?',
			workspace_id: 'tenant-1',
			scope
		});
	});

	it('opens citations through the authoritative backend contract', async () => {
		vi.mocked(apiClient.post).mockResolvedValue({ available: true });
		await openAskCitation('elembra://io.elembra.files/file/f-1');
		expect(apiClient.post).toHaveBeenCalledWith('/memory/citations/open', {
			resource_ref: 'elembra://io.elembra.files/file/f-1'
		});
	});

	it.each([
		[
			{ type: 'folder', resourceRef: 'elembra://io.elembra.files/folder/f-1' } satisfies AskScope,
			'/ask?scope=folder&resourceRef=elembra%3A%2F%2Fio.elembra.files%2Ffolder%2Ff-1'
		],
		[
			{ type: 'note', resourceRef: 'elembra://io.elembra.files/file/n-1' } satisfies AskScope,
			'/ask?scope=note&resourceRef=elembra%3A%2F%2Fio.elembra.files%2Ffile%2Fn-1'
		],
		[
			{
				type: 'chatChannel',
				communityId: 'community-1',
				channelId: 'channel-1'
			} satisfies AskScope,
			'/ask?scope=chat&communityId=community-1&channelId=channel-1'
		]
	])('builds the exact scoped entry URL %o', (scope, expected) => {
		expect(askHref(scope)).toBe(expected);
	});
});
