import { render, screen, waitFor, cleanup } from '@testing-library/svelte';
import { beforeEach, afterEach, describe, it, expect, vi } from 'vitest';
import ChannelPermissions from './ChannelPermissions.svelte';
const mocks = vi.hoisted(() => ({ query: vi.fn(), key: 'key' as string | null }));
vi.mock('$lib/chat/nostr', () => ({ queryRelayEvents: mocks.query }));
vi.mock('$lib/chat/session', () => ({ getSigningKey: () => mocks.key }));
const pubkey = 'a'.repeat(64);
function snapshot(role: string) {
	return {
		ok: true,
		events: [
			{
				id: '1',
				kind: 39000,
				created_at: 1,
				tags: [
					['d', 'channel'],
					['name', 'Test']
				]
			},
			{
				id: '2',
				kind: 39002,
				created_at: 1,
				tags: [
					['d', 'channel'],
					['p', pubkey, '', role]
				]
			}
		]
	};
}
function mount() {
	const changed = vi.fn();
	render(ChannelPermissions, {
		relayUrl: 'wss://relay.test',
		relayPubkey: 'b'.repeat(64),
		channelId: 'channel',
		pubkey,
		onPostingChange: changed
	});
	return changed;
}
beforeEach(() => {
	mocks.query.mockReset();
	mocks.key = 'key';
});
afterEach(cleanup);
describe('ChannelPermissions', () => {
	it('allows a confirmed member and denies a guest independently of readable metadata', async () => {
		mocks.query.mockResolvedValue(snapshot('guest'));
		const changed = mount();
		await screen.findByText('Posting unavailable for your current channel role.');
		expect(changed).not.toHaveBeenCalledWith(true);
		cleanup();
		mocks.query.mockResolvedValue(snapshot('member'));
		const memberChanged = mount();
		await waitFor(() => expect(memberChanged).toHaveBeenLastCalledWith(true));
	});
	it('fails closed after relay denial and when the key changes during a read', async () => {
		mocks.query.mockResolvedValue({ ok: false, reason: 'rejected' });
		const changed = mount();
		await screen.findByRole('alert');
		expect(changed).not.toHaveBeenCalledWith(true);
		cleanup();
		let resolve!: (value: unknown) => void;
		mocks.query.mockImplementation(() => new Promise((r) => (resolve = r)));
		const staleChanged = mount();
		mocks.key = null;
		resolve(snapshot('owner'));
		await Promise.resolve();
		expect(staleChanged).not.toHaveBeenCalledWith(true);
	});
});
