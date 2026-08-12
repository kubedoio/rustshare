import { render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ChatApplicationView from './ChatApplicationView.svelte';
import { queryClient } from '$lib/query-client';
import type { ChatStatus, ChatChannelInfo } from '$lib/api/chat';

const mocks = vi.hoisted(() => ({
	getChatStatus: vi.fn(),
	getChatChannels: vi.fn(),
	getChatMessages: vi.fn(),
	getChatMessage: vi.fn(),
	pageUrl: new URL('http://localhost:8080/apps/chat')
}));

vi.mock('$lib/api/chat', () => ({
	getChatStatus: mocks.getChatStatus,
	getChatChannels: mocks.getChatChannels,
	getChatMessages: mocks.getChatMessages,
	getChatMessage: mocks.getChatMessage
}));

vi.mock('$app/stores', () => ({
	page: readable({ url: mocks.pageUrl })
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({ tenant_id: 'tenant-1' })
}));

const CHANNELS: ChatChannelInfo[] = [
	{ channel_id: 'general', channel_kind: 'topic', latest_event_at: '2026-08-12T10:00:00Z' },
	{ channel_id: 'random', channel_kind: 'topic', latest_event_at: '2026-08-12T10:00:00Z' }
];

function activeStatus(overrides: Partial<ChatStatus> = {}): ChatStatus {
	return {
		chat_enabled: true,
		mapping: { community_id: 'community-1', relay_url: 'wss://relay.example' },
		binding: { status: 'Active', buzz_pubkey: 'pk-1' },
		admission_active: true,
		...overrides
	};
}

describe('ChatApplicationView', () => {
	beforeEach(() => {
		vi.mocked(mocks.getChatStatus).mockReset();
		vi.mocked(mocks.getChatChannels).mockReset();
		vi.mocked(mocks.getChatMessages).mockReset();
		vi.mocked(mocks.getChatMessage).mockReset();
		mocks.pageUrl.search = '';
		queryClient.clear();
	});

	it('shows the disabled state when chat is off for the workspace', async () => {
		mocks.getChatStatus.mockResolvedValue({
			chat_enabled: false,
			mapping: null,
			binding: null,
			admission_active: false
		});
		render(ChatApplicationView);
		await waitFor(() =>
			expect(screen.getByText('Chat is not enabled for this workspace.')).toBeTruthy()
		);
	});

	it('shows the mapping notice when no Buzz community is mapped', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ mapping: null, binding: null }));
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText(/No Buzz community is mapped/)).toBeTruthy());
	});

	it('renders the binding panel for an unbound user', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ binding: null }));
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText('Set up Chat')).toBeTruthy());
	});

	it('renders channel names for a bound, admitted user', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText(/general/)).toBeTruthy());
		expect(screen.getByText(/random/)).toBeTruthy();
	});

	it('fetches the deep-linked message', async () => {
		mocks.pageUrl.search = '?message=m-1';
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.getChatMessage.mockResolvedValue({
			message_id: 'm-1',
			event_id: 'e-1',
			community_id: 'community-1',
			channel_id: 'general',
			channel_kind: 'topic',
			author_pubkey: 'pk-a',
			event_created_at: '2026-08-12T10:00:00Z',
			thread_root_id: null,
			body: 'hello deep link'
		});
		render(ChatApplicationView);
		await waitFor(() => expect(mocks.getChatMessage).toHaveBeenCalledWith('m-1'));
	});
});
