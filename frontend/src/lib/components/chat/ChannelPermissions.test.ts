import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/svelte';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import ChannelPermissions from './ChannelPermissions.svelte';

const mocks = vi.hoisted(() => ({
	query: vi.fn(),
	key: 'key' as string | null,
	addChannelMember: vi.fn(),
	removeChannelMember: vi.fn(),
	editChannel: vi.fn(),
	joinChannel: vi.fn(),
	leaveChannel: vi.fn(),
	retryChannelMutation: vi.fn()
}));

vi.mock('$lib/chat/nostr', () => ({ queryRelayEvents: mocks.query }));
vi.mock('$lib/chat/session', () => ({ getSigningKey: () => mocks.key }));
vi.mock('$lib/chat/channels', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/chat/channels')>();
	return {
		...actual,
		addChannelMember: mocks.addChannelMember,
		removeChannelMember: mocks.removeChannelMember,
		editChannel: mocks.editChannel,
		joinChannel: mocks.joinChannel,
		leaveChannel: mocks.leaveChannel,
		retryChannelMutation: mocks.retryChannelMutation
	};
});

const pubkey = 'a'.repeat(64);
const other = 'c'.repeat(64);

interface SnapshotOptions {
	role?: string | null;
	visibility?: 'open' | 'private';
	others?: [string, string][];
	name?: string;
}

function snapshot({
	role = 'member',
	visibility = 'open',
	others = [],
	name = 'Test'
}: SnapshotOptions = {}) {
	const metadataTags: string[][] = [['name', name]];
	if (visibility === 'private') metadataTags.push(['private']);
	const pTags: string[][] = [];
	if (role !== null) pTags.push(['p', pubkey, '', role]);
	for (const [pk, r] of others) pTags.push(['p', pk, '', r]);
	return {
		ok: true,
		events: [
			{
				id: '1',
				kind: 39000,
				created_at: 1,
				tags: [['d', 'channel'], ...metadataTags]
			},
			{
				id: '2',
				kind: 39002,
				created_at: 1,
				tags: [['d', 'channel'], ...pTags]
			}
		]
	};
}

function mount() {
	const changed = vi.fn();
	const onChanged = vi.fn();
	render(ChannelPermissions, {
		relayUrl: 'wss://relay.test',
		relayPubkey: 'b'.repeat(64),
		channelId: 'channel',
		pubkey,
		onPostingChange: changed,
		onChanged
	});
	return { changed, onChanged };
}

async function openDetails() {
	await waitFor(() => expect(screen.queryByText('Checking channel permissions…')).toBeNull());
	await fireEvent.click(screen.getByRole('button', { name: 'Channel details' }));
	await screen.findByText(/Your role:/);
}

beforeEach(() => {
	mocks.query.mockReset();
	mocks.key = 'key';
	mocks.addChannelMember.mockReset();
	mocks.removeChannelMember.mockReset();
	mocks.editChannel.mockReset();
	mocks.joinChannel.mockReset();
	mocks.leaveChannel.mockReset();
	mocks.retryChannelMutation.mockReset();
});
afterEach(cleanup);

describe('ChannelPermissions', () => {
	it('allows a confirmed member and denies a guest independently of readable metadata', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'guest' }));
		const { changed } = mount();
		await screen.findByText('Posting unavailable for your current channel role.');
		expect(changed).not.toHaveBeenCalledWith(true);
		cleanup();
		mocks.query.mockResolvedValue(snapshot({ role: 'member' }));
		const memberChanged = mount();
		await waitFor(() => expect(memberChanged.changed).toHaveBeenLastCalledWith(true));
	});

	it('fails closed after relay denial and when the key changes during a read', async () => {
		mocks.query.mockResolvedValue({ ok: false, reason: 'rejected' });
		const { changed } = mount();
		await screen.findByRole('alert');
		expect(changed).not.toHaveBeenCalledWith(true);
		cleanup();
		let resolve!: (value: unknown) => void;
		mocks.query.mockImplementation(() => new Promise((r) => (resolve = r)));
		const staleChanged = mount();
		mocks.key = null;
		resolve(snapshot({ role: 'owner' }));
		await Promise.resolve();
		expect(staleChanged.changed).not.toHaveBeenCalledWith(true);
	});

	it('renders the roster with the actual roles the relay reports, including guest', async () => {
		mocks.query.mockResolvedValue(
			snapshot({
				role: 'owner',
				others: [
					[other, 'guest'],
					['d'.repeat(64), 'admin']
				]
			})
		);
		mount();
		await openDetails();
		expect(screen.getByText('Your role: owner')).toBeTruthy();
		expect(screen.getAllByText('guest').length).toBeGreaterThan(0);
		expect(screen.getAllByText('admin').length).toBeGreaterThan(0);
		expect(screen.getAllByText('owner').length).toBeGreaterThan(0);
	});

	it('hides member management from a plain member of a private channel', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'member', visibility: 'private' }));
		mount();
		await openDetails();
		expect(screen.queryByRole('button', { name: 'Add member' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Edit channel' })).toBeNull();
		expect(screen.queryByRole('button', { name: /Remove member/ })).toBeNull();
	});

	it('lets an owner add and remove members and surfaces relay rejections', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'owner', others: [[other, 'member']] }));
		mocks.addChannelMember.mockResolvedValue({
			status: 'rejected',
			detail: 'only owners/admins may add private-channel members'
		});
		const { onChanged } = mount();
		await openDetails();

		// Invalid pubkey is rejected locally without calling the relay path.
		await fireEvent.input(screen.getByLabelText('Member pubkey'), {
			target: { value: 'not-hex' }
		});
		expect((screen.getByRole('button', { name: 'Add member' }) as HTMLButtonElement).disabled).toBe(
			true
		);
		await fireEvent.input(screen.getByLabelText('Member pubkey'), {
			target: { value: 'e'.repeat(64) }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Add member' }));
		await screen.findByText(
			'Relay rejected the operation: only owners/admins may add private-channel members'
		);
		expect(onChanged).not.toHaveBeenCalled();

		// A confirmed removal updates the panel and notifies the parent.
		mocks.removeChannelMember.mockResolvedValue({ status: 'confirmed' });
		await fireEvent.click(screen.getByRole('button', { name: 'Remove member cccccccc' }));
		await screen.findByText('Member removed.');
		expect(mocks.removeChannelMember).toHaveBeenCalledWith(
			expect.objectContaining({ relayUrl: 'wss://relay.test', secretKey: 'key' }),
			'channel',
			other
		);
		await waitFor(() => expect(onChanged).toHaveBeenCalled());
	});

	it('keeps an uncertain add retryable with the same signed event identity', async () => {
		const event = { id: 'e-uncertain', kind: 9000 };
		mocks.query.mockResolvedValue(snapshot({ role: 'owner' }));
		mocks.addChannelMember.mockResolvedValue({ status: 'uncertain', event });
		mount();
		await openDetails();
		await fireEvent.input(screen.getByLabelText('Member pubkey'), {
			target: { value: 'e'.repeat(64) }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Add member' }));
		await screen.findByText(/acceptance is uncertain/);
		expect(screen.getByRole('button', { name: 'Retry same request' })).toBeTruthy();

		mocks.retryChannelMutation.mockResolvedValue({ status: 'confirmed' });
		await fireEvent.click(screen.getByRole('button', { name: 'Retry same request' }));
		await screen.findByText('Member added.');
		expect(mocks.retryChannelMutation).toHaveBeenCalledWith(
			expect.objectContaining({ secretKey: 'key' }),
			event,
			expect.any(Function)
		);
	});

	it('joins open channels when not a member and leaves when permitted', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: null }));
		mocks.joinChannel.mockResolvedValue({ status: 'confirmed' });
		mount();
		await openDetails();
		await fireEvent.click(screen.getByRole('button', { name: 'Join channel' }));
		await screen.findByText('Joined channel.');
		expect(mocks.joinChannel).toHaveBeenCalledWith(
			expect.objectContaining({ secretKey: 'key' }),
			'channel'
		);

		cleanup();
		mocks.query.mockResolvedValue(snapshot({ role: 'member' }));
		mocks.leaveChannel.mockResolvedValue({ status: 'confirmed' });
		mount();
		await openDetails();
		await fireEvent.click(screen.getByRole('button', { name: 'Leave channel' }));
		await screen.findByText('Left channel.');
	});

	it('refuses last-owner leave up front with the relay reason', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'owner' }));
		mount();
		await openDetails();
		expect(screen.queryByRole('button', { name: 'Leave channel' })).toBeNull();
		expect(screen.getByText(/a channel cannot lose its last owner/)).toBeTruthy();
		expect(mocks.leaveChannel).not.toHaveBeenCalled();
	});

	it('lets owners edit the name and description and confirms via relay state', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'admin' }));
		mocks.editChannel.mockResolvedValue({ status: 'confirmed' });
		const { onChanged } = mount();
		await openDetails();
		await fireEvent.click(screen.getByRole('button', { name: 'Edit channel' }));
		const nameInput = screen.getByLabelText('Channel name') as HTMLInputElement;
		expect(nameInput.value).toBe('Test');
		await fireEvent.input(nameInput, { target: { value: 'Renamed' } });
		await fireEvent.input(screen.getByLabelText('Channel description'), {
			target: { value: 'New purpose' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
		await screen.findByText('Channel details updated.');
		expect(mocks.editChannel).toHaveBeenCalledWith(
			expect.objectContaining({ secretKey: 'key' }),
			'channel',
			{ name: 'Renamed', description: 'New purpose' }
		);
		await waitFor(() => expect(onChanged).toHaveBeenCalled());
	});

	it('does not offer metadata editing to non-privileged roles', async () => {
		mocks.query.mockResolvedValue(snapshot({ role: 'member' }));
		mount();
		await openDetails();
		expect(screen.queryByRole('button', { name: 'Edit channel' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Add member' })).toBeTruthy();
	});
});
