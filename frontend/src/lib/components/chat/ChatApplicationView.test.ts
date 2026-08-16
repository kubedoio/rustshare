import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
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
	pageUrl: new URL('http://localhost:8080/apps/chat'),
	// Send path (real MessageComposer): key vault + nostr + files so the
	// composer can publish and emit onSent(eventId).
	hasChatKey: vi.fn(() => false),
	loadChatKey: vi.fn(async () => 'sk-1'),
	importChatKey: vi.fn(),
	exportChatKey: vi.fn(() => ''),
	clearChatKey: vi.fn(),
	publishEvent: vi.fn(),
	publicKeyOf: vi.fn(() => 'pk-1'),
	buildUnsignedEvent: vi.fn(
		async (kind: number, content: string, tags: unknown[], pubkey: string) => ({
			pubkey,
			created_at: 0,
			kind,
			tags,
			content
		})
	),
	listAllFiles: vi.fn(async () => [])
}));

vi.mock('$lib/api/chat', () => ({
	getChatStatus: mocks.getChatStatus,
	getChatChannels: mocks.getChatChannels,
	getChatMessages: mocks.getChatMessages,
	getChatMessage: mocks.getChatMessage
}));

vi.mock('$lib/chat/keys', () => ({
	hasChatKey: mocks.hasChatKey,
	loadChatKey: mocks.loadChatKey,
	importChatKey: mocks.importChatKey,
	exportChatKey: mocks.exportChatKey,
	clearChatKey: mocks.clearChatKey
}));

vi.mock('$lib/chat/nostr', () => ({
	publishEvent: mocks.publishEvent,
	publicKeyOf: mocks.publicKeyOf,
	buildUnsignedEvent: mocks.buildUnsignedEvent,
	isUuid: (value: string) =>
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value),
	NOSTR_KIND_STREAM_MESSAGE: 9,
	NOSTR_KIND_TEXT: 1
}));

vi.mock('$lib/api/files', () => ({ listAllFiles: mocks.listAllFiles }));

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
		ask_available: true,
		...overrides
	};
}

describe('ChatApplicationView', () => {
	beforeEach(() => {
		vi.mocked(mocks.getChatStatus).mockReset();
		vi.mocked(mocks.getChatChannels).mockReset();
		vi.mocked(mocks.getChatMessages).mockReset();
		vi.mocked(mocks.getChatMessage).mockReset();
		// Send-path mocks: restore defaults so the composer shows the import
		// UI unless a send test opts into a stored key.
		vi.mocked(mocks.hasChatKey).mockReset().mockReturnValue(false);
		vi.mocked(mocks.loadChatKey).mockReset().mockResolvedValue('sk-1');
		vi.mocked(mocks.publishEvent).mockReset();
		vi.mocked(mocks.buildUnsignedEvent)
			.mockReset()
			.mockImplementation(
				async (kind: number, content: string, tags: unknown[], pubkey: string) => ({
					pubkey,
					created_at: 0,
					kind,
					tags,
					content
				})
			);
		mocks.pageUrl.search = '';
		queryClient.clear();
	});

	it('shows the disabled state when chat is off for the workspace', async () => {
		mocks.getChatStatus.mockResolvedValue({
			chat_enabled: false,
			mapping: null,
			binding: null,
			admission_active: false,
			ask_available: false
		});
		render(ChatApplicationView);
		await waitFor(() =>
			expect(screen.getByText('Chat is not enabled for this workspace.')).toBeTruthy()
		);
	});

	it('shows the configuring notice when no community mapping exists', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ mapping: null, binding: null }));
		render(ChatApplicationView);
		await waitFor(() =>
			expect(screen.getByText(/Chat is being configured for this workspace/)).toBeTruthy()
		);
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

	it('does not advertise Ask when the provider is unavailable', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ ask_available: false }));
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText(/Ask this channel is unavailable/)).toBeTruthy());
		expect(screen.queryByRole('link', { name: 'Ask this channel' })).toBeNull();
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

	it('loads earlier pages with the next_before cursor', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockImplementation(async (channelId: string, before?: string | null) =>
			before === 't2'
				? { messages: [], next_before: null }
				: {
						messages: [
							{
								message_id: 'm-1',
								event_id: 'e-1',
								community_id: 'community-1',
								channel_id: 'general',
								channel_kind: 'topic',
								author_pubkey: 'pk-a',
								event_created_at: '2026-08-12T10:00:00Z',
								thread_root_id: null,
								body: 'first page message'
							}
						],
						next_before: 't2'
					}
		);
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText('Load earlier')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier' }));
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('general', 't2'));
	});

	it('shows a Back to latest control after paging and returns to the newest page', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockImplementation(async (channelId: string, before?: string | null) =>
			before === 't2'
				? { messages: [], next_before: null }
				: {
						messages: [
							{
								message_id: 'm-1',
								event_id: 'e-1',
								community_id: 'community-1',
								channel_id: 'general',
								channel_kind: 'topic',
								author_pubkey: 'pk-a',
								event_created_at: '2026-08-12T10:00:00Z',
								thread_root_id: null,
								body: 'first page message'
							}
						],
						next_before: 't2'
					}
		);
		render(ChatApplicationView);
		await waitFor(() => expect(screen.getByText('Load earlier')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier' }));
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Back to latest' })).toBeTruthy()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Back to latest' }));
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('general', null));
	});

	it('switching channels refetches messages for the new channel', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		render(ChatApplicationView);
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('general', null));
		await fireEvent.click(screen.getByRole('button', { name: /random/ }));
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('random', null));
	});

	it('clears the send-sync banner once the sent message is observed, then auto-hides it', async () => {
		vi.useFakeTimers();
		try {
			mocks.getChatStatus.mockResolvedValue(activeStatus());
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			mocks.hasChatKey.mockReturnValue(true);
			mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e-sent-1' });
			render(ChatApplicationView);
			await waitFor(() => expect(screen.getByPlaceholderText(/Message #general/)).toBeTruthy());

			await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
				target: { value: 'hello relay' }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
			await waitFor(() =>
				expect(screen.getByText('Sent — waiting for Elembra sync…')).toBeTruthy()
			);

			// The polling fallback picks the sent event up: the status flips to
			// 'observed'… then the success banner auto-clears after ~3 s.
			mocks.getChatMessages.mockResolvedValue({
				messages: [
					{
						message_id: 'm-sent',
						event_id: 'e-sent-1',
						community_id: 'community-1',
						channel_id: 'general',
						channel_kind: 'topic',
						author_pubkey: 'pk-1',
						event_created_at: '2026-08-12T10:01:00Z',
						thread_root_id: null,
						body: 'hello relay'
					}
				],
				next_before: null
			});
			await vi.advanceTimersByTimeAsync(15_000);
			await waitFor(() => expect(screen.getByText('Observed by Elembra.')).toBeTruthy());

			await vi.advanceTimersByTimeAsync(3_000);
			await waitFor(() => expect(screen.queryByText('Observed by Elembra.')).toBeNull());
		} finally {
			vi.useRealTimers();
		}
	});

	it('resets the send-sync status when switching channels mid-wait', async () => {
		vi.useFakeTimers();
		try {
			mocks.getChatStatus.mockResolvedValue(activeStatus());
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			mocks.hasChatKey.mockReturnValue(true);
			mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e-sent-2' });
			render(ChatApplicationView);
			await waitFor(() => expect(screen.getByPlaceholderText(/Message #general/)).toBeTruthy());

			await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
				target: { value: 'about to switch' }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
			await waitFor(() =>
				expect(screen.getByText('Sent — waiting for Elembra sync…')).toBeTruthy()
			);

			// Switching channels orphans the in-flight event: the status resets
			// to idle and the stale 15 s warning timer must not fire for it.
			await fireEvent.click(screen.getByRole('button', { name: /random/ }));
			await waitFor(() =>
				expect(screen.queryByText('Sent — waiting for Elembra sync…')).toBeNull()
			);
			await vi.advanceTimersByTimeAsync(20_000);
			expect(screen.queryByText(/Sent, but Elembra has not observed/)).toBeNull();
		} finally {
			vi.useRealTimers();
		}
	});

	it('refetches channels on the polling fallback so a dead websocket cannot freeze the channel list', async () => {
		vi.useFakeTimers();
		try {
			mocks.getChatStatus.mockResolvedValue(activeStatus());
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			render(ChatApplicationView);
			await waitFor(() => expect(screen.getByText(/general/)).toBeTruthy());
			const channelsCalls = mocks.getChatChannels.mock.calls.length;
			const messagesCalls = mocks.getChatMessages.mock.calls.length;
			const statusCalls = mocks.getChatStatus.mock.calls.length;
			await vi.advanceTimersByTimeAsync(16_000);
			await waitFor(() =>
				expect(mocks.getChatChannels.mock.calls.length).toBeGreaterThan(channelsCalls)
			);
			expect(mocks.getChatMessages.mock.calls.length).toBeGreaterThan(messagesCalls);
			expect(mocks.getChatStatus.mock.calls.length).toBeGreaterThan(statusCalls);
		} finally {
			vi.useRealTimers();
		}
	});
});
