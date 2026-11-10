import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ThreadView from './ThreadView.svelte';
import type { NostrEvent } from '$lib/chat/nostr';

const rootId = '1'.repeat(64);
const alice = 'a'.repeat(64);
const bob = 'b'.repeat(64);

function event(id: string, content: string, tags: string[][], created_at: number): NostrEvent {
	return { id, kind: 9, pubkey: alice, tags, content, created_at, sig: '' };
}

const root = event(rootId, 'root', [['h', '11111111-1111-4111-8111-111111111111']], 1);
const directReply = event('2'.repeat(64), 'direct reply', [['e', rootId, '', 'reply']], 2);
const nestedReply = event(
	'3'.repeat(64),
	'nested reply',
	[
		['e', rootId, '', 'root'],
		['e', directReply.id, '', 'reply']
	],
	3
);
// References the root WITHOUT thread markers (e.g. a quote-style mention):
// the #e query returns it, but it is not part of the thread ancestry.
const markerlessReference = event('4'.repeat(64), 'just mentioning it', [['e', rootId]], 4);
// A message from a different thread that shares no tags with this root.
const foreign = event(
	'5'.repeat(64),
	'other thread',
	[
		['e', '9'.repeat(64), '', 'root'],
		['e', '9'.repeat(64), '', 'reply']
	],
	5
);

function renderView(events: NostrEvent[]) {
	return render(ThreadView, {
		props: {
			rootId,
			rootSummary: null,
			events,
			loading: false,
			error: false,
			viewerPubkey: bob,
			reactions: new Map(),
			deletedTargets: new Map(),
			reactionsAvailable: false,
			canDelete: () => null,
			busyTarget: null,
			onClose: vi.fn(),
			onReply: vi.fn(),
			onReact: vi.fn(),
			onDelete: vi.fn()
		}
	});
}

describe('ThreadView grouping', () => {
	it('renders only events belonging to the thread ancestry, oldest first', () => {
		renderView([foreign, nestedReply, markerlessReference, root, directReply]);
		expect(screen.getByText('root')).toBeTruthy();
		expect(screen.getByText('direct reply')).toBeTruthy();
		expect(screen.getByText('nested reply')).toBeTruthy();
		expect(screen.queryByText('just mentioning it')).toBeNull();
		expect(screen.queryByText('other thread')).toBeNull();
		// Root first, then replies in relay order.
		const log = screen.getByRole('log', { name: 'Thread messages' });
		const text = log.textContent ?? '';
		expect(text.indexOf('root')).toBeLessThan(text.indexOf('direct reply'));
		expect(text.indexOf('direct reply')).toBeLessThan(text.indexOf('nested reply'));
	});

	it('shows the thread-empty state when the root exists but has no replies', () => {
		renderView([root, foreign]);
		expect(screen.getByText('root')).toBeTruthy();
		expect(screen.getByText('0 replies')).toBeTruthy();
		expect(screen.queryByText('other thread')).toBeNull();
	});
});
