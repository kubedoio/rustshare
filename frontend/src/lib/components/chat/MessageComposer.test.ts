import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MessageComposer from './MessageComposer.svelte';

const mocks = vi.hoisted(() => {
	// Storeful fake of the key vault so tests can move between "no key on this
	// device" (import UI) and "key present" (composer + export) states.
	let stored: { raw: string } | null = null;
	return {
		hasChatKey: vi.fn(() => stored !== null),
		loadChatKey: vi.fn(async (passphrase: string) => {
			if (passphrase !== 'pass') throw new Error('wrong passphrase');
			return 'sk-imported';
		}),
		importChatKey: vi.fn(async (json: string, passphrase: string) => {
			if (!json.trim() || passphrase !== 'pass') throw new Error('decrypt failed');
			stored = { raw: json };
			return 'sk-imported';
		}),
		exportChatKey: vi.fn(() => stored?.raw ?? ''),
		publishEvent: vi.fn(),
		publicKeyOf: vi.fn(() => 'pk-1'),
		buildUnsignedEvent: vi.fn(
			async (_kind: number, content: string, _tags: unknown[], _pubkey: string) => ({
				pubkey: 'pk-1',
				created_at: 0,
				kind: 1,
				tags: [],
				content
			})
		),
		clear: () => {
			stored = null;
		}
	};
});

vi.mock('$lib/chat/keys', () => ({
	hasChatKey: mocks.hasChatKey,
	loadChatKey: mocks.loadChatKey,
	importChatKey: mocks.importChatKey,
	exportChatKey: mocks.exportChatKey
}));

vi.mock('$lib/chat/nostr', () => ({
	publishEvent: mocks.publishEvent,
	publicKeyOf: mocks.publicKeyOf,
	buildUnsignedEvent: mocks.buildUnsignedEvent,
	NOSTR_KIND_TEXT: 1
}));

vi.mock('$lib/api/files', () => ({ listAllFiles: vi.fn(async () => []) }));

function renderComposer(onSendFailure = vi.fn()) {
	return {
		onSendFailure,
		...render(MessageComposer, {
			relayUrl: 'wss://relay.example',
			channelId: 'general',
			boundPubkey: 'pk-1',
			onSendFailure
		})
	};
}

describe('MessageComposer', () => {
	beforeEach(() => {
		vi.mocked(mocks.publishEvent).mockReset().mockResolvedValue({ ok: true });
		vi.mocked(mocks.publicKeyOf).mockReset().mockReturnValue('pk-1');
		vi.mocked(mocks.buildUnsignedEvent)
			.mockReset()
			.mockImplementation(
				async (_kind: number, content: string, _tags: unknown[], _pubkey: string) => ({
					pubkey: 'pk-1',
					created_at: 0,
					kind: 1,
					tags: [],
					content
				})
			);
		vi.mocked(mocks.hasChatKey).mockClear();
		vi.mocked(mocks.loadChatKey).mockClear();
		vi.mocked(mocks.importChatKey).mockClear();
		vi.mocked(mocks.exportChatKey).mockClear();
		mocks.clear();
	});

	it('shows the import UI when the device holds no key, imports, then sends as the bound identity', async () => {
		const { onSendFailure } = renderComposer();
		await waitFor(() => expect(screen.getByText(/No chat key on this device/)).toBeTruthy());

		const backup = '{"v":1,"salt":"aa","iv":"bb","ciphertext":"cc","pubkey":"pk-1"}';
		await fireEvent.input(screen.getByLabelText('Key backup'), { target: { value: backup } });
		await fireEvent.input(screen.getByPlaceholderText('backup passphrase'), {
			target: { value: 'pass' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));

		await waitFor(() => expect(mocks.importChatKey).toHaveBeenCalledWith(backup, 'pass'));
		await waitFor(() => expect(screen.queryByText(/No chat key on this device/)).toBeNull());

		await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
			target: { value: 'hello from the second device' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send' }));

		await waitFor(() =>
			expect(mocks.publishEvent).toHaveBeenCalledWith(
				'wss://relay.example',
				expect.objectContaining({ pubkey: 'pk-1', content: 'hello from the second device' }),
				'sk-imported'
			)
		);
		expect(onSendFailure).toHaveBeenCalledWith('');
	});

	it('surfaces an import failure for a bad backup or passphrase', async () => {
		const { onSendFailure } = renderComposer();
		await waitFor(() => expect(screen.getByText(/No chat key on this device/)).toBeTruthy());

		await fireEvent.input(screen.getByLabelText('Key backup'), {
			target: { value: 'not-a-json-backup' }
		});
		await fireEvent.input(screen.getByPlaceholderText('backup passphrase'), {
			target: { value: 'wrong' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Import key' }));

		await waitFor(() => expect(screen.getByText('decrypt failed')).toBeTruthy());
		expect(mocks.publishEvent).not.toHaveBeenCalled();
		expect(onSendFailure).not.toHaveBeenCalled();
	});

	it('reports a relay rejection with the relay reason', async () => {
		mocks.hasChatKey.mockReturnValue(true);
		// The unlock flow itself is covered at the unit level (keys.test.ts);
		// here the key is already unlocked so sends reach publishEvent.
		vi.mocked(mocks.loadChatKey).mockResolvedValue('sk-imported');
		vi.mocked(mocks.publishEvent).mockResolvedValue({
			ok: false,
			reason: 'rejected',
			detail: 'blocked: not admitted'
		});
		const { onSendFailure } = renderComposer();
		await waitFor(() => expect(screen.queryByText(/No chat key on this device/)).toBeNull());
		await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
			target: { value: 'will be rejected' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'relay rejected the message: blocked: not admitted'
			)
		);
	});

	it('reports a transport failure as relay unreachable', async () => {
		mocks.hasChatKey.mockReturnValue(true);
		vi.mocked(mocks.loadChatKey).mockResolvedValue('sk-imported');
		vi.mocked(mocks.publishEvent).mockResolvedValue({ ok: false, reason: 'transport' });
		const { onSendFailure } = renderComposer();
		await waitFor(() => expect(screen.queryByText(/No chat key on this device/)).toBeNull());
		await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
			target: { value: 'relay is down' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
		await waitFor(() => expect(onSendFailure).toHaveBeenCalledWith('relay unreachable'));
	});

	it('does not publish when the local key pubkey differs from the bound identity', async () => {
		mocks.hasChatKey.mockReturnValue(true);
		vi.mocked(mocks.loadChatKey).mockResolvedValue('sk-other');
		vi.mocked(mocks.publicKeyOf).mockReturnValue('pk-other');
		const { onSendFailure } = renderComposer();
		await waitFor(() => expect(screen.queryByText(/No chat key on this device/)).toBeNull());
		await fireEvent.input(screen.getByPlaceholderText(/Message #general/), {
			target: { value: 'identity mismatch' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
		await waitFor(() =>
			expect(onSendFailure).toHaveBeenCalledWith(
				'local key does not match your bound Buzz identity'
			)
		);
		expect(mocks.publishEvent).not.toHaveBeenCalled();
	});

	it('exposes key export when a local key exists', async () => {
		mocks.hasChatKey.mockReturnValue(true);
		const clipboard = { writeText: vi.fn().mockResolvedValue(undefined) };
		Object.assign(navigator, { clipboard });
		const { onSendFailure } = renderComposer();
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Export key backup' })).toBeTruthy()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Export key backup' }));
		await waitFor(() => expect(mocks.exportChatKey).toHaveBeenCalled());
		await waitFor(() => expect(clipboard.writeText).toHaveBeenCalled());
		expect(onSendFailure).not.toHaveBeenCalled();
	});
});
