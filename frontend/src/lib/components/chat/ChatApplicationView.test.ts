import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import ChatApplicationView from './ChatApplicationView.svelte';
import { queryClient } from '$lib/query-client';
import type { ChatStatus, ChatChannelInfo } from '$lib/api/chat';

const mocks = vi.hoisted(() => {
	const subscribers = new Set<(value: unknown) => void>();
	let currentSessionValue: unknown = { state: 'locked' };
	const sessionState = {
		subscribe(fn: (value: unknown) => void) {
			fn(currentSessionValue);
			subscribers.add(fn);
			return () => subscribers.delete(fn);
		},
		set(value: unknown) {
			currentSessionValue = value;
			subscribers.forEach((fn) => fn(value));
		}
	};
	return {
		getChatStatus: vi.fn(),
		getChatChannels: vi.fn(),
		getChatMessages: vi.fn(),
		getChatMessage: vi.fn(),
		openChatAttachment: vi.fn(),
		pageUrl: new URL('http://localhost:8080/apps/chat'),
		sessionState,
		signingKey: null as string | null,
		hasChatKey: false,
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
		listAllFiles: vi.fn(async () => []),
		apiClient: {
			get: vi.fn(),
			post: vi.fn(),
			getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
		}
	};
});

vi.mock('$lib/api/chat', () => ({
	getChatStatus: mocks.getChatStatus,
	getChatChannels: mocks.getChatChannels,
	getChatMessages: mocks.getChatMessages,
	getChatMessage: mocks.getChatMessage,
	openChatAttachment: mocks.openChatAttachment
}));

vi.mock('$lib/api/client', () => ({
	apiClient: mocks.apiClient
}));

vi.mock('$lib/chat/session', () => ({
	chatSessionStore: { subscribe: mocks.sessionState.subscribe },
	getSigningKey: () => mocks.signingKey,
	unlock: vi.fn(async () => {
		mocks.signingKey = 'sk-1';
		mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
	}),
	lock: vi.fn(() => {
		mocks.signingKey = null;
		mocks.sessionState.set({ state: 'locked' });
	}),
	clear: vi.fn(() => {
		mocks.signingKey = null;
		mocks.sessionState.set({ state: 'locked' });
	})
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

vi.mock('$lib/chat/keys', () => ({
	hasChatKey: () => mocks.hasChatKey,
	importChatKey: vi.fn(),
	clearChatKey: vi.fn()
}));

vi.mock('$app/stores', () => ({
	page: readable({ url: mocks.pageUrl })
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({ tenant_id: 'tenant-1' })
}));

const CHANNELS: ChatChannelInfo[] = [
	{
		channel_id: 'general',
		name: 'general',
		channel_kind: 'topic',
		channel_type: null,
		visibility: null,
		member: null,
		latest_event_at: '2026-08-12T10:00:00Z'
	},
	{
		channel_id: 'random',
		name: 'random',
		channel_kind: 'topic',
		channel_type: null,
		visibility: null,
		member: null,
		latest_event_at: '2026-08-12T10:00:00Z'
	}
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

function renderView() {
	return render(ChatApplicationView);
}

describe('ChatApplicationView', () => {
	beforeEach(() => {
		vi.mocked(mocks.getChatStatus).mockReset();
		vi.mocked(mocks.getChatChannels).mockReset();
		vi.mocked(mocks.getChatMessages).mockReset();
		vi.mocked(mocks.getChatMessage).mockReset();
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
		mocks.sessionState.set({ state: 'locked' });
		mocks.signingKey = null;
		mocks.hasChatKey = false;
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
		renderView();
		await waitFor(() =>
			expect(screen.getByText('Chat is not enabled for this workspace.')).toBeTruthy()
		);
	});

	it('shows the configuring notice when no community mapping exists', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ mapping: null, binding: null }));
		renderView();
		await waitFor(() =>
			expect(screen.getByText(/Chat is being configured for this workspace/)).toBeTruthy()
		);
		expect(screen.queryByText('Set up Chat')).toBeNull();
	});

	it('renders the binding panel for an unbound user', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ binding: null }));
		renderView();
		await waitFor(() => expect(screen.getByText('Set up Chat')).toBeTruthy());
	});

	it('never shows the configuring notice when a mapping exists but the user is not bound', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ binding: null }));
		renderView();
		await waitFor(() => expect(screen.getByText('Set up Chat')).toBeTruthy());
		expect(screen.queryByText(/Chat is being configured for this workspace/)).toBeNull();
	});

	it('renders channel names for a bound, admitted user', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		renderView();
		await waitFor(() => expect(screen.getByRole('option', { name: /general/ })).toBeTruthy());
		expect(screen.getByRole('option', { name: /random/ })).toBeTruthy();
	});

	it('shows the unlock panel when the Chat session is locked', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.hasChatKey = true;
		renderView();
		await waitFor(() => expect(screen.getByText('Unlock Chat')).toBeTruthy());
		expect(screen.queryByRole('button', { name: 'Send message' })).toBeNull();
	});

	it('renders the composer when the Chat session is unlocked', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.hasChatKey = true;
		mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
		mocks.signingKey = 'sk-1';
		renderView();
		await waitFor(() => expect(screen.getByRole('button', { name: 'Send message' })).toBeTruthy());
		expect(screen.queryByText('Unlock Chat')).toBeNull();
	});

	it('sends a message without re-asking for the passphrase when already unlocked', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e-sent-1' });
		mocks.hasChatKey = true;
		mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
		mocks.signingKey = 'sk-1';
		renderView();

		await waitFor(() => expect(screen.getByLabelText('Message text')).toBeTruthy());
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'hello relay' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() => expect(mocks.publishEvent).toHaveBeenCalled());
		await waitFor(() => expect(screen.getByText('Sent — waiting for Elembra sync…')).toBeTruthy());
	});

	it('advertises Ask Elembra when the provider is available', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ ask_available: true }));
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		renderView();
		await waitFor(() => expect(screen.getByRole('link', { name: 'Ask Elembra' })).toBeTruthy());
	});

	it('disables Ask Elembra when the provider is unavailable', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus({ ask_available: false }));
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		renderView();
		await waitFor(() => {
			const askButton = screen.getByRole('button', { name: 'Ask Elembra' });
			expect(askButton).toBeTruthy();
			expect(askButton.hasAttribute('disabled')).toBe(true);
		});
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
			author: null,
			event_created_at: '2026-08-12T10:00:00Z',
			thread_root_id: null,
			attachments: [],
			body: 'hello deep link'
		});
		renderView();
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
								author: null,
								event_created_at: '2026-08-12T10:00:00Z',
								thread_root_id: null,
								attachments: [],
								body: 'first page message'
							}
						],
						next_before: 't2'
					}
		);
		renderView();
		await waitFor(() => expect(screen.getByText('Load earlier messages')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier messages' }));
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
								author: null,
								event_created_at: '2026-08-12T10:00:00Z',
								thread_root_id: null,
								attachments: [],
								body: 'first page message'
							}
						],
						next_before: 't2'
					}
		);
		renderView();
		await waitFor(() => expect(screen.getByText('Load earlier messages')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier messages' }));
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
		renderView();
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('general', null));
		await fireEvent.click(screen.getByRole('option', { name: /random/ }));
		await waitFor(() => expect(mocks.getChatMessages).toHaveBeenCalledWith('random', null));
	});

	it('clears the send-sync banner once the sent message is observed, then auto-hides it', async () => {
		vi.useFakeTimers();
		try {
			mocks.getChatStatus.mockResolvedValue(activeStatus());
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e-sent-1' });
			mocks.hasChatKey = true;
			mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
			mocks.signingKey = 'sk-1';
			renderView();
			await waitFor(() => expect(screen.getByLabelText('Message text')).toBeTruthy());

			await fireEvent.input(screen.getByLabelText('Message text'), {
				target: { value: 'hello relay' }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
			await waitFor(() =>
				expect(screen.getByText('Sent — waiting for Elembra sync…')).toBeTruthy()
			);

			mocks.getChatMessages.mockResolvedValue({
				messages: [
					{
						message_id: 'm-sent',
						event_id: 'e-sent-1',
						community_id: 'community-1',
						channel_id: 'general',
						channel_kind: 'topic',
						author_pubkey: 'pk-1',
						author: null,
						event_created_at: '2026-08-12T10:01:00Z',
						thread_root_id: null,
						attachments: [],
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
			mocks.publishEvent.mockResolvedValue({ ok: true, event_id: 'e-sent-2' });
			mocks.hasChatKey = true;
			mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
			mocks.signingKey = 'sk-1';
			renderView();
			await waitFor(() => expect(screen.getByLabelText('Message text')).toBeTruthy());

			await fireEvent.input(screen.getByLabelText('Message text'), {
				target: { value: 'about to switch' }
			});
			await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
			await waitFor(() =>
				expect(screen.getByText('Sent — waiting for Elembra sync…')).toBeTruthy()
			);

			await fireEvent.click(screen.getByRole('option', { name: /random/ }));
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
			renderView();
			await waitFor(() => expect(screen.getByRole('option', { name: /general/ })).toBeTruthy());
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
