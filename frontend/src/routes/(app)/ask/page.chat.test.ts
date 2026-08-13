import { render, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import AskPage from './+page.svelte';
import AskExperience from '$lib/components/ask/AskExperience.svelte';

const mocks = vi.hoisted(() => ({
	pageUrl: new URL('http://localhost:8080/ask'),
	goto: vi.fn()
}));

vi.mock('$app/stores', () => ({
	page: readable({ url: mocks.pageUrl })
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/components/ask/AskExperience.svelte', () => ({
	default: vi.fn(() => null)
}));

describe('Ask page chat wiring', () => {
	beforeEach(() => {
		vi.mocked(AskExperience).mockClear();
		mocks.goto.mockReset();
		mocks.pageUrl.search = '';
	});

	it('parses the chat scope from the URL', async () => {
		mocks.pageUrl.search = '?scope=chat&communityId=community-1&channelId=channel-1';
		render(AskPage);
		await waitFor(() => expect(AskExperience).toHaveBeenCalled());
		const props = vi.mocked(AskExperience).mock.calls[0][1] as {
			scope: unknown;
			onChatCitationOpen?: (citation: unknown) => void;
		};
		expect(props.scope).toEqual({
			type: 'chatChannel',
			communityId: 'community-1',
			channelId: 'channel-1'
		});
		expect(typeof props.onChatCitationOpen).toBe('function');
	});

	it('navigates to the chat deep link for a chat citation', async () => {
		render(AskPage);
		await waitFor(() => expect(AskExperience).toHaveBeenCalled());
		const props = vi.mocked(AskExperience).mock.calls[0][1] as {
			onChatCitationOpen?: (citation: unknown) => void;
		};
		props.onChatCitationOpen?.({
			resource_ref: 'elembra://io.elembra.chat/message/abc',
			display_name: 'message',
			available: true
		});
		expect(mocks.goto).toHaveBeenCalledWith('/apps/chat?message=abc');
	});

	it('ignores citations that are not chat message refs', async () => {
		render(AskPage);
		await waitFor(() => expect(AskExperience).toHaveBeenCalled());
		const props = vi.mocked(AskExperience).mock.calls[0][1] as {
			onChatCitationOpen?: (citation: unknown) => void;
		};
		props.onChatCitationOpen?.({
			resource_ref: 'elembra://io.elembra.files/file/f-1',
			display_name: 'file',
			available: true
		});
		expect(mocks.goto).not.toHaveBeenCalled();
	});
});
