import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/svelte';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import CreateChannelPanel from './CreateChannelPanel.svelte';

const mocks = vi.hoisted(() => ({
	key: 'key' as string | null,
	createChannel: vi.fn(),
	retryChannelMutation: vi.fn()
}));

vi.mock('$lib/chat/session', () => ({ getSigningKey: () => mocks.key }));
vi.mock('$lib/chat/channels', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/chat/channels')>();
	return {
		...actual,
		createChannel: mocks.createChannel,
		retryChannelMutation: mocks.retryChannelMutation
	};
});

beforeEach(() => {
	mocks.key = 'key';
	mocks.createChannel.mockReset();
	mocks.retryChannelMutation.mockReset();
});
afterEach(cleanup);

async function openForm() {
	await fireEvent.click(screen.getByRole('button', { name: '+ New channel' }));
}

function fillForm(name: string, description = '') {
	const nameInput = screen.getByLabelText('New channel name');
	return fireEvent.input(nameInput, { target: { value: name } }).then(() => {
		if (description)
			return fireEvent.input(screen.getByLabelText('New channel description'), {
				target: { value: description }
			});
	});
}

describe('CreateChannelPanel', () => {
	it('creates a channel and reports the confirmed channel id', async () => {
		mocks.createChannel.mockResolvedValue({
			channelId: '11111111-1111-4111-8111-111111111111',
			result: { status: 'confirmed' }
		});
		const onCreated = vi.fn();
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated
		});
		await openForm();
		await fillForm('General', 'Team talk');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await waitFor(() =>
			expect(onCreated).toHaveBeenCalledWith('11111111-1111-4111-8111-111111111111')
		);
		expect(mocks.createChannel).toHaveBeenCalledWith(
			expect.objectContaining({ relayUrl: 'wss://relay.test', secretKey: 'key' }),
			{ name: 'General', description: 'Team talk', visibility: 'open' }
		);
		// A confirmed create closes the form.
		expect(screen.queryByLabelText('New channel name')).toBeNull();
	});

	it('creates private channels when selected', async () => {
		mocks.createChannel.mockResolvedValue({
			channelId: '11111111-1111-4111-8111-111111111111',
			result: { status: 'confirmed' }
		});
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated: () => {}
		});
		await openForm();
		await fillForm('Secret');
		await fireEvent.change(screen.getByLabelText('New channel visibility'), {
			target: { value: 'private' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await waitFor(() => expect(mocks.createChannel).toHaveBeenCalled());
		expect(mocks.createChannel.mock.calls[0][1].visibility).toBe('private');
	});

	it('validates the name locally before publishing', async () => {
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated: () => {}
		});
		await openForm();
		await fillForm('   ');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await screen.findByText('Channel name is required.');
		await fillForm('#prefixed');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await screen.findByText('Channel names must not start with #.');
		expect(mocks.createChannel).not.toHaveBeenCalled();
	});

	it('surfaces relay rejections such as a channel-id collision', async () => {
		mocks.createChannel.mockResolvedValue({
			channelId: '11111111-1111-4111-8111-111111111111',
			result: { status: 'rejected', detail: 'duplicate: channel already exists' }
		});
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated: () => {}
		});
		await openForm();
		await fillForm('Again');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await screen.findByText('Relay rejected the channel: duplicate: channel already exists');
	});

	it('keeps an uncertain create retryable with the same signed event identity', async () => {
		const event = { id: 'e-create', kind: 9007 };
		mocks.createChannel.mockResolvedValue({
			channelId: '11111111-1111-4111-8111-111111111111',
			result: { status: 'uncertain', event }
		});
		const onCreated = vi.fn();
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated
		});
		await openForm();
		await fillForm('Maybe');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await screen.findByText(/Creation is uncertain/);
		expect(screen.getByRole('button', { name: 'Retry same request' })).toBeTruthy();

		mocks.retryChannelMutation.mockResolvedValue({ status: 'confirmed' });
		await fireEvent.click(screen.getByRole('button', { name: 'Retry same request' }));
		await waitFor(() => expect(onCreated).toHaveBeenCalled());
		expect(mocks.retryChannelMutation).toHaveBeenCalledWith(
			expect.objectContaining({ secretKey: 'key' }),
			event,
			expect.any(Function)
		);
		expect(mocks.createChannel).toHaveBeenCalledTimes(1);
	});

	it('reports transport failures as offline feedback', async () => {
		mocks.createChannel.mockResolvedValue({
			channelId: '11111111-1111-4111-8111-111111111111',
			result: { status: 'transport' }
		});
		render(CreateChannelPanel, {
			relayUrl: 'wss://relay.test',
			relayPubkey: 'b'.repeat(64),
			onCreated: () => {}
		});
		await openForm();
		await fillForm('Offline');
		await fireEvent.click(screen.getByRole('button', { name: 'Create channel' }));
		await screen.findByText('Relay unreachable. Retry when connected.');
	});
});
