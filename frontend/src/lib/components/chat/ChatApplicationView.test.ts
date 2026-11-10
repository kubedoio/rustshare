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
		publishSignedEvent: vi.fn(),
		queryRelayEvents: vi.fn(),
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
	publishSignedEvent: mocks.publishSignedEvent,
	queryRelayEvents: mocks.queryRelayEvents,
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
		runtime_profile: 'local_client_held',
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
		vi.mocked(mocks.publishSignedEvent).mockReset();
		vi.mocked(mocks.queryRelayEvents).mockReset();
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

	it.each([null, { status: 'Active', buzz_pubkey: 'pk-2' }])(
		'locks the old signing session when binding changes to %s',
		async (binding) => {
			mocks.getChatStatus.mockResolvedValue(activeStatus());
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			mocks.hasChatKey = true;
			mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
			mocks.signingKey = 'sk-1';
			renderView();
			await waitFor(() => expect(screen.getByLabelText('Message text')).toBeTruthy());
			queryClient.setQueryData(['chat-status'], activeStatus({ binding }));
			await waitFor(() => expect(mocks.signingKey).toBeNull());
			expect(screen.queryByLabelText('Message text')).toBeNull();
		}
	);

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

	it.each([undefined, 'upstream_managed'] as const)(
		'does not authorize posting for profile %s',
		async (runtime_profile) => {
			mocks.getChatStatus.mockResolvedValue(activeStatus({ runtime_profile }));
			mocks.getChatChannels.mockResolvedValue(CHANNELS);
			mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
			mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
			mocks.signingKey = 'sk-1';
			renderView();
			await screen.findByLabelText('Message text');
			await fireEvent.input(screen.getByLabelText('Message text'), {
				target: { value: 'blocked' }
			});
			expect(
				(screen.getByRole('button', { name: 'Send message' }) as HTMLButtonElement).disabled
			).toBe(true);
			expect(mocks.publishEvent).not.toHaveBeenCalled();
		}
	);

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

	it('loads earlier pages with the next_before cursor and renders the older messages', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		mocks.getChatMessages.mockImplementation(async (channelId: string, before?: string | null) =>
			before === 't2'
				? {
						messages: [
							{
								message_id: 'm-0',
								event_id: 'e-0',
								community_id: 'community-1',
								channel_id: 'general',
								channel_kind: 'topic',
								author_pubkey: 'pk-a',
								author: null,
								event_created_at: '2026-08-12T09:00:00Z',
								thread_root_id: null,
								attachments: [],
								body: 'older page message'
							}
						],
						next_before: null
					}
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
		// The older page must actually reach the timeline — not just be fetched.
		await waitFor(() => expect(screen.getByText('older page message')).toBeTruthy());
		expect(screen.getByText('first page message')).toBeTruthy();
		// Page 2's next_before is null: the oldest page is reached, so the
		// control must disappear.
		await waitFor(() =>
			expect(screen.queryByRole('button', { name: 'Load earlier messages' })).toBeNull()
		);
	});

	it('retries a failed older-page fetch when Load earlier is clicked again', async () => {
		mocks.getChatStatus.mockResolvedValue(activeStatus());
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		let pageTwoFailures = 0;
		let pageTwoAllowed = false;
		mocks.getChatMessages.mockImplementation(async (channelId: string, before?: string | null) => {
			if (before === 't2') {
				if (!pageTwoAllowed) {
					pageTwoFailures += 1;
					throw new Error('network down');
				}
				return {
					messages: [
						{
							message_id: 'm-0',
							event_id: 'e-0',
							community_id: 'community-1',
							channel_id: 'general',
							channel_kind: 'topic',
							author_pubkey: 'pk-a',
							author: null,
							event_created_at: '2026-08-12T09:00:00Z',
							thread_root_id: null,
							attachments: [],
							body: 'older page message'
						}
					],
					next_before: null
				};
			}
			return {
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
			};
		});
		renderView();
		await waitFor(() => expect(screen.getByText('Load earlier messages')).toBeTruthy());
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier messages' }));
		// retry:1 plus our explicit refetch means the failed page is attempted
		// twice before the query settles as errored.
		await waitFor(
			() => {
				expect(pageTwoFailures).toBeGreaterThanOrEqual(2);
			},
			{ timeout: 5000 }
		);
		// The control stays visible after the failure and a second click retries.
		await waitFor(() => expect(screen.getByText('Load earlier messages')).toBeTruthy(), {
			timeout: 5000
		});
		pageTwoAllowed = true;
		await fireEvent.click(screen.getByRole('button', { name: 'Load earlier messages' }));
		await waitFor(() => expect(screen.getByText('older page message')).toBeTruthy(), {
			timeout: 5000
		});
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

describe('ChatApplicationView relay directory (fork_client_held)', () => {
	const EMPTY_CHANNEL_ID = '11111111-1111-4111-8111-111111111111';
	const PRIVATE_CHANNEL_ID = '22222222-2222-4222-8222-222222222222';

	function directoryEvent(channelId: string, name: string, extraTags: string[][] = []) {
		return {
			id: `m-${channelId}`,
			kind: 39000,
			created_at: 2,
			pubkey: 'relay-pubkey',
			tags: [['d', channelId], ['name', name], ...extraTags],
			content: '',
			sig: ''
		};
	}

	function channelStateEvents(channelId: string) {
		return [
			directoryEvent(channelId, 'Relay channel'),
			{
				id: 'r-1',
				kind: 39002,
				created_at: 2,
				pubkey: 'relay-pubkey',
				tags: [
					['d', channelId],
					['p', 'pk-1', '', 'member']
				],
				content: '',
				sig: ''
			}
		];
	}

	function forkStatus(overrides: Partial<ChatStatus> = {}): ChatStatus {
		return activeStatus({
			runtime_profile: 'fork_client_held',
			mapping: {
				community_id: 'community-1',
				relay_url: 'wss://relay.example',
				relay_pubkey: 'relay-pubkey'
			},
			...overrides
		});
	}

	function unlockSession() {
		mocks.hasChatKey = true;
		mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-1' });
		mocks.signingKey = 'sk-1';
	}

	function useDirectory(directoryEvents: unknown[], extra: Record<string, unknown> = {}): void {
		mocks.getChatStatus.mockResolvedValue(forkStatus());
		mocks.getChatChannels.mockResolvedValue([]);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.queryRelayEvents.mockImplementation(async (_url: string, filter: { '#d'?: string[] }) => {
			if (filter['#d']) return { ok: true, events: channelStateEvents(filter['#d'][0]) };
			return { ok: true, events: directoryEvents, ...extra };
		});
	}

	beforeEach(() => {
		vi.mocked(mocks.getChatStatus).mockReset();
		vi.mocked(mocks.getChatChannels).mockReset();
		vi.mocked(mocks.getChatMessages).mockReset();
		vi.mocked(mocks.getChatMessage).mockReset();
		vi.mocked(mocks.publishEvent).mockReset();
		vi.mocked(mocks.publishSignedEvent).mockReset();
		vi.mocked(mocks.queryRelayEvents).mockReset();
		mocks.sessionState.set({ state: 'locked' });
		mocks.signingKey = null;
		mocks.hasChatKey = false;
		mocks.pageUrl.search = '';
		queryClient.clear();
	});

	it('lists relay directory channels that have no observed messages', async () => {
		useDirectory([
			directoryEvent(EMPTY_CHANNEL_ID, 'Empty room'),
			directoryEvent(PRIVATE_CHANNEL_ID, 'Private room', [['private']])
		]);
		unlockSession();
		renderView();
		await waitFor(() => expect(screen.getByRole('option', { name: /Empty room/ })).toBeTruthy());
		expect(screen.getByRole('option', { name: /Private room/ })).toBeTruthy();
		expect(screen.getByLabelText('Private channel')).toBeTruthy();
		// The relay discovery query is the fork's kinds-[39000] REQ.
		expect(mocks.queryRelayEvents).toHaveBeenCalledWith(
			'wss://relay.example',
			{ kinds: [39000] },
			'sk-1',
			'relay-pubkey'
		);
	});

	it('keeps a deep link to an empty relay channel and reconstructs it on reload', async () => {
		useDirectory([directoryEvent(EMPTY_CHANNEL_ID, 'Empty room')]);
		unlockSession();
		mocks.pageUrl.search = `?channel=${EMPTY_CHANNEL_ID}`;
		const first = renderView();
		await waitFor(
			() =>
				expect(
					screen.getByRole('option', { name: /Empty room/ }).getAttribute('aria-selected')
				).toBe('true'),
			{ timeout: 3000 }
		);
		first.unmount();

		// Reload: no local state survives, so selection must rebuild from the
		// relay directory alone.
		queryClient.clear();
		mocks.pageUrl.search = `?channel=${EMPTY_CHANNEL_ID}`;
		renderView();
		await waitFor(
			() =>
				expect(
					screen.getByRole('option', { name: /Empty room/ }).getAttribute('aria-selected')
				).toBe('true'),
			{ timeout: 3000 }
		);
	});

	it('makes channel selection addressable in the URL', async () => {
		useDirectory([]);
		unlockSession();
		mocks.getChatChannels.mockResolvedValue(CHANNELS);
		const replace = vi.spyOn(history, 'replaceState');
		try {
			renderView();
			await waitFor(() => expect(screen.getByRole('option', { name: /random/ })).toBeTruthy());
			await fireEvent.click(screen.getByRole('option', { name: /random/ }));
			await waitFor(() =>
				expect(replace.mock.calls.some((call) => String(call[2]).includes('channel=random'))).toBe(
					true
				)
			);
		} finally {
			replace.mockRestore();
		}
	});

	it('never renders channels the relay does not disclose, including hidden DMs', async () => {
		useDirectory([
			directoryEvent(EMPTY_CHANNEL_ID, 'Public room'),
			directoryEvent('33333333-3333-4333-8333-333333333333', 'Secret DM', [['hidden']])
		]);
		unlockSession();
		renderView();
		await waitFor(() => expect(screen.getByRole('option', { name: /Public room/ })).toBeTruthy());
		// Access control happened at the relay: anything it did not return
		// (other workspaces' private channels, DMs) must not appear here.
		expect(screen.queryByText('Secret DM')).toBeNull();
		expect(screen.queryByText('Someone else’s private channel')).toBeNull();
	});

	it('excludes archived channels from the relay directory listing', async () => {
		useDirectory([
			directoryEvent(EMPTY_CHANNEL_ID, 'Active room'),
			directoryEvent('44444444-4444-4444-8444-444444444444', 'Archived room', [
				['archived', 'true']
			])
		]);
		unlockSession();
		renderView();
		await waitFor(() => expect(screen.getByRole('option', { name: /Active room/ })).toBeTruthy());
		expect(screen.queryByText('Archived room')).toBeNull();
	});

	it('ignores a deep-link channel param that is not a relay-assigned UUID', async () => {
		useDirectory([directoryEvent(EMPTY_CHANNEL_ID, 'Empty room')]);
		unlockSession();
		mocks.pageUrl.search = '?channel=../../not-a-channel-id';
		renderView();
		// The invalid param is ignored rather than trusted as a selection: no
		// messages query is ever issued for it, and selection rebuilds from
		// the directory like a normal load.
		await waitFor(
			() =>
				expect(
					screen.getByRole('option', { name: /Empty room/ }).getAttribute('aria-selected')
				).toBe('true'),
			{ timeout: 3000 }
		);
		expect(mocks.getChatMessages).not.toHaveBeenCalledWith('../../not-a-channel-id', null);
	});

	it('scopes the directory cache to the bound identity — B never sees A’s cached entries', async () => {
		// Identity A (sk-1) has a private channel; identity B (sk-2) on the same
		// relay must never be served A's cached directory from the TanStack
		// cache: the query key carries the session pubkey, so the identity
		// switch refetches under a fresh key.
		mocks.getChatStatus.mockResolvedValue(forkStatus());
		mocks.getChatChannels.mockResolvedValue([]);
		mocks.getChatMessages.mockResolvedValue({ messages: [], next_before: null });
		mocks.queryRelayEvents.mockImplementation(
			async (_url: string, filter: { '#d'?: string[] }, secretKey: string) => {
				if (filter['#d']) return { ok: true, events: channelStateEvents(filter['#d'][0]) };
				if (secretKey === 'sk-1')
					return {
						ok: true,
						events: [
							directoryEvent(PRIVATE_CHANNEL_ID, 'A private room', [['private']]),
							directoryEvent(EMPTY_CHANNEL_ID, 'A open room')
						]
					};
				return { ok: true, events: [directoryEvent(EMPTY_CHANNEL_ID, 'B open room')] };
			}
		);
		unlockSession();
		const first = renderView();
		await waitFor(() =>
			expect(screen.getByRole('option', { name: /A private room/ })).toBeTruthy()
		);
		first.unmount();

		// Same relay, same page lifetime, different bound identity (fresh
		// browser profile / re-bind). B's directory must come from B's own
		// relay query — never from A's cached private entries.
		queryClient.setQueryData(
			['chat-status'],
			forkStatus({ binding: { status: 'Active', buzz_pubkey: 'pk-2' } })
		);
		mocks.signingKey = 'sk-2';
		mocks.sessionState.set({ state: 'unlocked', pubkey: 'pk-2' });
		renderView();
		await waitFor(() => expect(screen.getByRole('option', { name: /B open room/ })).toBeTruthy());
		expect(screen.queryByRole('option', { name: /A private room/ })).toBeNull();
		expect(screen.queryByRole('option', { name: /A open room/ })).toBeNull();
		await waitFor(() =>
			expect(mocks.queryRelayEvents).toHaveBeenCalledWith(
				'wss://relay.example',
				{ kinds: [39000] },
				'sk-2',
				'relay-pubkey'
			)
		);
	});

	describe('permission refresh vs thread state', () => {
		const THREAD_CHANNEL = '33333333-3333-4333-8333-333333333333';
		// parseChannelState only counts 64-hex pubkeys in relay-signed rosters,
		// so the viewer must present a real hex pubkey for membership to parse.
		const VIEWER = 'f1'.repeat(32);
		// Mutable snapshot so individual tests can flip the relay answer between
		// refreshes; read at call time by the mocked transport.
		let permissionResult: { ok: true; events: unknown[] } | { ok: false; reason: string };

		function permissionSnapshot(member: boolean) {
			const roster = [
				directoryEvent(THREAD_CHANNEL, 'Thread channel'),
				{
					id: 'r-thread',
					kind: 39002,
					created_at: 2,
					pubkey: 'relay-pubkey',
					tags: member
						? [
								['d', THREAD_CHANNEL],
								['p', VIEWER, '', 'member']
							]
						: [['d', THREAD_CHANNEL]],
					content: '',
					sig: ''
				}
			];
			return { ok: true as const, events: roster };
		}

		function threadMessages() {
			const base = {
				community_id: 'community-1',
				channel_id: THREAD_CHANNEL,
				channel_kind: 'stream',
				author: { display_name: 'Bob', avatar_url: null, is_current_user: false },
				event_created_at: '2026-11-09T10:00:00Z',
				attachments: []
			};
			return {
				messages: [
					{
						...base,
						message_id: 'm-root',
						event_id: 'e-root',
						author_pubkey: 'pk-9',
						thread_root_id: null,
						body: 'root body'
					},
					{
						...base,
						message_id: 'm-reply',
						event_id: 'e-reply',
						author_pubkey: 'pk-9',
						thread_root_id: 'e-root',
						body: 'reply body'
					}
				],
				next_before: null
			};
		}

		function installRelayDispatcher() {
			mocks.queryRelayEvents.mockImplementation(
				async (_url: string, filter: { '#d'?: string[] }) =>
					filter['#d'] ? permissionResult : { ok: true, events: [] }
			);
		}

		async function openThread() {
			await fireEvent.click(screen.getByRole('button', { name: 'View thread, 1 reply' }));
			await screen.findByRole('heading', { name: 'Thread' });
		}

		function setup(member = true) {
			useDirectory([]);
			mocks.getChatStatus.mockResolvedValue(
				forkStatus({ binding: { status: 'Active', buzz_pubkey: VIEWER } })
			);
			mocks.getChatChannels.mockResolvedValue([
				{
					channel_id: THREAD_CHANNEL,
					name: 'Thread channel',
					channel_kind: 'stream',
					channel_type: 'stream',
					visibility: 'open',
					member: true,
					latest_event_at: '2026-08-12T10:00:00Z'
				}
			]);
			mocks.getChatMessages.mockResolvedValue(threadMessages());
			permissionResult = permissionSnapshot(member);
			installRelayDispatcher();
			unlockSession();
			mocks.sessionState.set({ state: 'unlocked', pubkey: VIEWER });
			renderView();
		}

		it('keeps an open thread across a routine refresh with unchanged permissions', async () => {
			setup();
			await screen.findByRole('button', { name: 'View thread, 1 reply' });
			await openThread();
			await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
			await waitFor(() => expect(screen.getByRole('heading', { name: 'Thread' })).toBeTruthy());
			expect(screen.getByText('root body')).toBeTruthy();
		});

		it('clears the open thread when the refresh CONFIRMS revocation', async () => {
			setup();
			await screen.findByRole('button', { name: 'View thread, 1 reply' });
			await openThread();
			permissionResult = permissionSnapshot(false);
			await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
			await waitFor(() =>
				expect(screen.getByText('Posting unavailable for your current channel role.')).toBeTruthy()
			);
			expect(screen.queryByRole('heading', { name: 'Thread' })).toBeNull();
			// Re-admission alone must not resurrect the cleared thread…
			permissionResult = permissionSnapshot(true);
			await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
			await waitFor(() =>
				expect(screen.queryByText('Posting unavailable for your current channel role.')).toBeNull()
			);
			expect(screen.queryByRole('heading', { name: 'Thread' })).toBeNull();
			// …but the thread is re-openable from the timeline badge.
			expect(screen.getByRole('button', { name: 'View thread, 1 reply' })).toBeTruthy();
		});

		it('keeps thread state across a transient (unknown) refresh outcome', async () => {
			setup();
			await screen.findByRole('button', { name: 'View thread, 1 reply' });
			await openThread();
			// Transport failure: posting goes fail-closed but the denial is not
			// confirmed, so the open thread must survive and come back once a
			// refresh confirms membership again.
			permissionResult = { ok: false, reason: 'transport' };
			await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
			await waitFor(() => expect(screen.getByText(/Channel permissions unavailable/)).toBeTruthy());
			permissionResult = permissionSnapshot(true);
			await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
			await waitFor(() => expect(screen.getByRole('heading', { name: 'Thread' })).toBeTruthy());
			expect(screen.getByText('root body')).toBeTruthy();
		});
	});
});
