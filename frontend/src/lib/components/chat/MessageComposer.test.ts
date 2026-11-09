import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MessageComposer from './MessageComposer.svelte';

const mocks = vi.hoisted(() => {
	const attachmentTag = ['a', 'file', 'io.elembra.files', 'file', 'abc', null] as (string | null)[];
	return {
		getSigningKey: vi.fn((): string | null => 'sk-1'),
		publishEvent: vi.fn(),
		publishSignedEvent: vi.fn(),
		publicKeyOf: vi.fn(() => 'pk-1'),
		buildUnsignedEvent: vi.fn(
			async (_kind: number, content: string, _tags: unknown[], _pubkey: string) => ({
				pubkey: 'pk-1',
				created_at: 0,
				kind: 9,
				tags: [],
				content
			})
		),
		attachmentTag,
		listAllFiles: vi.fn(async () => [
			{ id: 'f-1', name: 'plan.txt', path: '/plan.txt', mime_type: 'text/plain', size: 12 }
		]),
		apiPost: vi.fn(async () => ({ buzz_tag: attachmentTag }))
	};
});

vi.mock('$lib/chat/session', () => ({
	getSigningKey: mocks.getSigningKey,
	chatSessionStore: {
		subscribe: vi.fn((fn: (value: unknown) => void) => fn({ state: 'unlocked' }))
	}
}));

vi.mock('$lib/chat/nostr', () => ({
	publishEvent: mocks.publishEvent,
	publishSignedEvent: mocks.publishSignedEvent,
	publicKeyOf: mocks.publicKeyOf,
	buildUnsignedEvent: mocks.buildUnsignedEvent,
	isUuid: (value: string) =>
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value),
	NOSTR_KIND_STREAM_MESSAGE: 9,
	NOSTR_KIND_TEXT: 1
}));

vi.mock('$lib/api/files', () => ({ listAllFiles: mocks.listAllFiles }));

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		post: mocks.apiPost,
		postVoid: vi.fn(),
		requestBlob: vi.fn(),
		getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
	}
}));

function renderComposer(
	props: {
		onSendFailure?: (message: string) => void;
		channelId?: string;
		channelName?: string;
		boundPubkey?: string;
		signingKey?: string | null;
		postingAllowed?: boolean;
		replyTarget?: { eventId: string; rootId?: string | null } | null;
		onCancelReply?: () => void;
	} = {}
) {
	const {
		onSendFailure = vi.fn(),
		channelId = 'general',
		channelName = 'general',
		boundPubkey = 'pk-1',
		signingKey = 'sk-1',
		postingAllowed = true,
		replyTarget = null,
		onCancelReply = vi.fn()
	} = props;
	mocks.getSigningKey.mockReturnValue(signingKey);
	return {
		onSendFailure,
		onCancelReply,
		...render(MessageComposer, {
			relayUrl: 'wss://relay.example',
			channelId,
			channelName,
			boundPubkey,
			postingAllowed,
			replyTarget,
			onCancelReply,
			onSendFailure
		})
	};
}

describe('MessageComposer', () => {
	beforeEach(() => {
		sessionStorage.clear();
		vi.mocked(mocks.publishSignedEvent)
			.mockReset()
			.mockResolvedValue({ ok: true, event_id: 'same-event' });
		vi.mocked(mocks.getSigningKey).mockReset().mockReturnValue('sk-1');
		vi.mocked(mocks.publishEvent).mockReset().mockResolvedValue({ ok: true });
		vi.mocked(mocks.publicKeyOf).mockReset().mockReturnValue('pk-1');
		vi.mocked(mocks.buildUnsignedEvent)
			.mockReset()
			.mockImplementation(
				async (_kind: number, content: string, _tags: unknown[], _pubkey: string) => ({
					pubkey: 'pk-1',
					created_at: 0,
					kind: 9,
					tags: [],
					content
				})
			);
		vi.mocked(mocks.apiPost).mockReset().mockResolvedValue({ buzz_tag: mocks.attachmentTag });
	});

	it('publishes a kind-9 stream message with the h tag when the channel id is a UUID', async () => {
		const channelUuid = '11111111-2222-4333-8444-555555555555';
		const { onSendFailure } = renderComposer({ channelId: channelUuid, channelName: 'ops' });

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'scoped hello' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() =>
			expect(mocks.buildUnsignedEvent).toHaveBeenCalledWith(
				9,
				'scoped hello',
				[['h', channelUuid]],
				'pk-1'
			)
		);
		expect(onSendFailure).toHaveBeenCalledWith('');
	});

	it('falls back to kind-1 for name-based channels', async () => {
		const { onSendFailure } = renderComposer({ channelId: 'general', channelName: 'general' });

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'hello general' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() =>
			expect(mocks.buildUnsignedEvent).toHaveBeenCalledWith(1, 'hello general', [], 'pk-1')
		);
		expect(onSendFailure).toHaveBeenCalledWith('');
	});

	it('disables send when locked (no signing key) or empty', async () => {
		renderComposer({ signingKey: null });
		const lockedSendButton = screen.getByRole('button', {
			name: 'Send message'
		}) as HTMLButtonElement;
		expect(lockedSendButton.disabled).toBe(true);

		cleanup();
		renderComposer({ signingKey: 'sk-1' });
		const emptySendButton = screen.getByRole('button', {
			name: 'Send message'
		}) as HTMLButtonElement;
		expect(emptySendButton.disabled).toBe(true);
	});

	it('shows the channel name in the textarea placeholder', () => {
		renderComposer({ channelId: 'general', channelName: 'random' });
		const textarea = screen.getByLabelText('Message text') as HTMLTextAreaElement;
		expect(textarea.placeholder).toBe('Message #random');
	});

	it('sends on Enter and inserts a newline on Shift+Enter', async () => {
		const { onSendFailure } = renderComposer();
		const textarea = screen.getByLabelText('Message text');

		await fireEvent.input(textarea, { target: { value: 'send me' } });
		await fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false });

		await waitFor(() => expect(mocks.publishEvent).toHaveBeenCalled());
		expect(onSendFailure).toHaveBeenCalledWith('');

		await fireEvent.input(textarea, { target: { value: 'line one' } });
		await fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: true });
		// Shift+Enter should not trigger a send.
		const callsAfterShiftEnter = mocks.publishEvent.mock.calls.length;
		expect(callsAfterShiftEnter).toBe(1);
	});

	it('prevents double-send while a message is in flight', async () => {
		let resolvePublish: (value: { ok: true; event_id: string }) => void = () => {};
		vi.mocked(mocks.publishEvent).mockImplementation(
			() =>
				new Promise((resolve) => {
					resolvePublish = resolve;
				})
		);
		renderComposer();

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'first' }
		});
		const sendButton = screen.getByRole('button', { name: 'Send message' }) as HTMLButtonElement;
		await fireEvent.click(sendButton);
		expect(sendButton.disabled).toBe(true);

		// A second click during the in-flight publish must not fire again.
		await fireEvent.click(sendButton);
		expect(mocks.publishEvent).toHaveBeenCalledTimes(1);

		resolvePublish({ ok: true, event_id: 'e-1' });
		// After a successful send the draft is cleared, so the button remains
		// disabled until the user types again. The important invariant is that
		// only one publish happened.
		await waitFor(() => expect(mocks.publishEvent).toHaveBeenCalledTimes(1));
	});

	it('reports a relay rejection with the relay reason', async () => {
		vi.mocked(mocks.publishEvent).mockResolvedValue({
			ok: false,
			reason: 'rejected',
			detail: 'blocked: not admitted'
		});
		const { onSendFailure } = renderComposer();

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'will be rejected' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'Relay rejected the message: blocked: not admitted'
			)
		);
	});

	it('reports a transport failure as relay unreachable', async () => {
		vi.mocked(mocks.publishEvent).mockResolvedValue({ ok: false, reason: 'transport' });
		const { onSendFailure } = renderComposer();

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'relay is down' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() => expect(onSendFailure).toHaveBeenCalledWith('Relay unreachable'));
	});

	it('reports a local key that does not match the bound identity', async () => {
		vi.mocked(mocks.publicKeyOf).mockReturnValue('pk-other');
		const { onSendFailure } = renderComposer();

		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'identity mismatch' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'Local Chat key does not match your bound Buzz identity.'
			)
		);
		expect(mocks.publishEvent).not.toHaveBeenCalled();
	});

	it('sends an attachment-only message', async () => {
		const { onSendFailure } = renderComposer();

		const sendButton = screen.getByRole('button', { name: 'Send message' }) as HTMLButtonElement;
		expect(sendButton.disabled).toBe(true);

		await fireEvent.click(screen.getByRole('button', { name: 'Attach file' }));
		await waitFor(() => expect(screen.getByText('plan.txt')).toBeTruthy());
		await fireEvent.click(screen.getByText('plan.txt'));

		await waitFor(() => expect(sendButton.disabled).toBe(false));
		await fireEvent.click(sendButton);

		await waitFor(() =>
			expect(mocks.buildUnsignedEvent).toHaveBeenCalledWith(
				1,
				'',
				expect.arrayContaining([mocks.attachmentTag]),
				'pk-1'
			)
		);
		expect(onSendFailure).toHaveBeenCalledWith('');
	});
	it('denies posting without an explicit channel permission', async () => {
		renderComposer({ postingAllowed: false });
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'not authorized' }
		});
		expect(
			(screen.getByRole('button', { name: 'Send message' }) as HTMLButtonElement).disabled
		).toBe(true);
		await fireEvent.keyDown(screen.getByLabelText('Message text'), { key: 'Enter' });
		expect(mocks.publishEvent).not.toHaveBeenCalled();
	});

	it('preserves channel-scoped drafts across remount without sharing them to another channel', async () => {
		const first = renderComposer({ channelId: 'first' });
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'first draft' }
		});
		first.unmount();
		const second = renderComposer({ channelId: 'second' });
		expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe('');
		second.unmount();
		renderComposer({ channelId: 'first' });
		await waitFor(() =>
			expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe(
				'first draft'
			)
		);
	});

	it('retains an uncertain signed message across reload and retries the exact event', async () => {
		const event = {
			id: 'same-event',
			pubkey: 'pk-1',
			kind: 1,
			created_at: 123,
			content: 'maybe sent',
			tags: [],
			sig: 'signature'
		};
		mocks.publishEvent.mockResolvedValue({
			ok: false,
			reason: 'uncertain',
			event_id: event.id,
			event
		});
		const first = renderComposer();
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'maybe sent' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
		await screen.findByRole('button', { name: 'Retry same message' });
		expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).disabled).toBe(true);
		first.unmount();
		renderComposer();
		await fireEvent.click(await screen.findByRole('button', { name: 'Retry same message' }));
		await waitFor(() =>
			expect(mocks.publishSignedEvent).toHaveBeenCalledWith('wss://relay.example', event, 'sk-1')
		);
		expect(mocks.publishEvent).toHaveBeenCalledTimes(1);
		await waitFor(() =>
			expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe('')
		);
	});

	it('persists the signed event before the first send and refuses to send when storage fails', async () => {
		mocks.publishEvent.mockImplementation(async (_url, _event, _sk, onSigned) => {
			onSigned({ id: 'pending', pubkey: 'pk-1', kind: 1, tags: [] });
			throw new Error('should not reach network after persistence error');
		});
		const { onSendFailure } = renderComposer();
		await fireEvent.input(screen.getByLabelText('Message text'), { target: { value: 'draft' } });
		const storage = vi.spyOn(sessionStorage, 'setItem').mockImplementation(() => {
			throw new Error('quota exceeded');
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'Cannot preserve the signed message safely. Browser storage is unavailable.'
			)
		);
		storage.mockRestore();
	});

	it('publishes a reply with canonical NIP-10 root/reply tags', async () => {
		const channelUuid = '11111111-2222-4333-8444-555555555555';
		const rootId = 'a1'.repeat(32);
		const parentId = 'b2'.repeat(32);
		const { onSendFailure } = renderComposer({
			channelId: channelUuid,
			replyTarget: { eventId: parentId, rootId }
		});

		expect(screen.getByText(/Replying in thread/)).toBeTruthy();
		await fireEvent.input(screen.getByLabelText('Message text'), { target: { value: 'a reply' } });
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));

		await waitFor(() =>
			expect(mocks.buildUnsignedEvent).toHaveBeenCalledWith(
				9,
				'a reply',
				[
					['h', channelUuid],
					['e', rootId, '', 'root'],
					['e', parentId, '', 'reply']
				],
				'pk-1'
			)
		);
		expect(onSendFailure).toHaveBeenCalledWith('');
	});

	it('never publishes a reply whose thread parent is malformed', async () => {
		const { onSendFailure } = renderComposer({
			channelId: '11111111-2222-4333-8444-555555555555',
			replyTarget: { eventId: 'not-hex' }
		});
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'bad reply' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send message' }));
		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'Reply target is invalid. Cancel the reply and try again.'
			)
		);
		expect(mocks.publishEvent).not.toHaveBeenCalled();
	});

	it('scopes drafts per thread and keeps them apart from the channel draft', async () => {
		const channelUuid = '11111111-2222-4333-8444-555555555555';
		const rootId = 'a1'.repeat(32);
		const otherRootId = 'c3'.repeat(32);

		const channelLevel = renderComposer({ channelId: channelUuid });
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'channel draft' }
		});
		channelLevel.unmount();

		const inThread = renderComposer({
			channelId: channelUuid,
			replyTarget: { eventId: rootId, rootId }
		});
		await fireEvent.input(screen.getByLabelText('Message text'), {
			target: { value: 'thread draft' }
		});
		inThread.unmount();

		renderComposer({
			channelId: channelUuid,
			replyTarget: { eventId: otherRootId, rootId: otherRootId }
		});
		expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe('');
		cleanup();
		renderComposer({ channelId: channelUuid, replyTarget: { eventId: rootId, rootId } });
		await waitFor(() =>
			expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe(
				'thread draft'
			)
		);
		cleanup();
		renderComposer({ channelId: channelUuid });
		await waitFor(() =>
			expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe(
				'channel draft'
			)
		);
	});

	it('reads legacy three-element draft keys as channel-level drafts', async () => {
		const channelUuid = '11111111-2222-4333-8444-555555555555';
		const legacyKey = `elembra.chat.draft:${JSON.stringify(['wss://relay.example', 'pk-1', channelUuid])}`;
		sessionStorage.setItem(
			legacyKey,
			JSON.stringify({ draft: 'legacy draft', attachmentTag: null, pendingEvent: null })
		);
		renderComposer({ channelId: channelUuid });
		await waitFor(() =>
			expect((screen.getByLabelText('Message text') as HTMLTextAreaElement).value).toBe(
				'legacy draft'
			)
		);
		// Migration: the legacy key is folded into the new four-element key and
		// deleted, so clearing the new draft cannot resurrect the old one.
		const newKey = `elembra.chat.draft:${JSON.stringify(['wss://relay.example', 'pk-1', channelUuid, null])}`;
		expect(sessionStorage.getItem(legacyKey)).toBeNull();
		expect(JSON.parse(sessionStorage.getItem(newKey) ?? '{}').draft).toBe('legacy draft');
	});

	it('invokes onCancelReply from the reply banner', async () => {
		const { onCancelReply } = renderComposer({
			channelId: '11111111-2222-4333-8444-555555555555',
			replyTarget: { eventId: 'a1'.repeat(32) }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Cancel reply' }));
		expect(onCancelReply).toHaveBeenCalled();
	});
});
