import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import MessageTimeline from './MessageTimeline.svelte';
import type { ChatMessageDto } from '$lib/api/chat';

const mocks = vi.hoisted(() => ({
	openChatAttachment: vi.fn(),
	copyToClipboard: vi.fn()
}));

vi.mock('$lib/api/chat', () => ({
	openChatAttachment: mocks.openChatAttachment
}));

vi.mock('$lib/utils/clipboard', () => ({
	copyToClipboard: mocks.copyToClipboard
}));

function message(overrides: Partial<ChatMessageDto> = {}): ChatMessageDto {
	return {
		message_id: 'm-1',
		event_id: 'e-1',
		community_id: 'community-1',
		channel_id: 'general',
		channel_kind: 'topic',
		author_pubkey: 'pk-a',
		author: null,
		event_created_at: '2026-08-12T10:00:00Z',
		thread_root_id: null,
		body: 'hello',
		attachments: [],
		...overrides
	};
}

function renderTimeline(messages: ChatMessageDto[], focusTarget: ChatMessageDto | null = null) {
	return render(MessageTimeline, {
		props: {
			messages,
			loading: false,
			focusTarget,
			onLoadMore: vi.fn(),
			askAvailable: true,
			communityId: 'community-1'
		}
	});
}

describe('MessageTimeline focus scroll', () => {
	let scrollSpy: ReturnType<typeof vi.fn>;

	beforeEach(() => {
		scrollSpy = vi.fn();
		HTMLElement.prototype.scrollIntoView = scrollSpy as unknown as () => void;
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('scrolls when the focused message is already rendered', () => {
		const target = message();
		renderTimeline([target], target);
		expect(screen.getByText('hello')).toBeTruthy();
		expect(scrollSpy).toHaveBeenCalledTimes(1);
	});

	it('scrolls once the focused message arrives after the initial page load', async () => {
		const { rerender } = renderTimeline([]);
		// The focus query resolves before the channel page fetch: focusTarget is
		// set while the timeline is still empty — no scroll yet.
		const target = message();
		await rerender({ messages: [], focusTarget: target });
		expect(scrollSpy).not.toHaveBeenCalled();
		// The page arrives: the effect re-runs on `messages` and scrolls.
		await rerender({ messages: [target], focusTarget: target });
		expect(scrollSpy).toHaveBeenCalledTimes(1);
	});

	it('highlights the focused message row', () => {
		const target = message();
		renderTimeline([target], target);
		const row = document.querySelector(`[data-message-id="${target.message_id}"]`);
		expect(row?.className).toContain('bg-primary/10');
	});
});

describe('MessageTimeline message rendering', () => {
	it('shows the author display name when present', () => {
		renderTimeline([
			message({
				author: { display_name: 'Ada Lovelace', avatar_url: null, is_current_user: false }
			})
		]);
		expect(screen.getByText('Ada Lovelace')).toBeTruthy();
	});

	it('falls back to unknown author label when author is missing', () => {
		renderTimeline([message({ author: null })]);
		expect(screen.getByText('Unknown Buzz user')).toBeTruthy();
	});

	it('shows a shortened author pubkey hint', () => {
		renderTimeline([message({ author_pubkey: 'abcdef1234567890' })]);
		expect(screen.getByText('abcdef12')).toBeTruthy();
	});

	it('shows a formatted local timestamp with full ISO in title', () => {
		renderTimeline([message({ event_created_at: '2026-08-12T20:04:00Z' })]);
		const time = screen.getByTitle('2026-08-12T20:04:00Z');
		expect(time).toBeTruthy();
		expect(time.tagName).toBe('TIME');
	});

	it('renders a date separator when messages span days', () => {
		const today = new Date().toISOString();
		const yesterdayDate = new Date();
		yesterdayDate.setDate(yesterdayDate.getDate() - 1);
		const yesterday = yesterdayDate.toISOString();
		renderTimeline([
			message({ message_id: 'm-1', event_id: 'e-1', event_created_at: yesterday }),
			message({ message_id: 'm-2', event_id: 'e-2', event_created_at: today })
		]);
		expect(screen.getByText('Today')).toBeTruthy();
		expect(screen.getByText('Yesterday')).toBeTruthy();
	});

	it('groups consecutive messages from the same author within 5 minutes', () => {
		renderTimeline([
			message({
				message_id: 'm-1',
				event_id: 'e-1',
				author: { display_name: 'Ada', avatar_url: null, is_current_user: false },
				event_created_at: '2026-08-12T10:00:00Z',
				body: 'first'
			}),
			message({
				message_id: 'm-2',
				event_id: 'e-2',
				author: { display_name: 'Ada', avatar_url: null, is_current_user: false },
				author_pubkey: 'pk-a',
				event_created_at: '2026-08-12T10:02:00Z',
				body: 'second'
			})
		]);
		// Only the first grouped message shows the author name.
		const names = screen.getAllByText('Ada');
		expect(names).toHaveLength(1);
		expect(screen.getByText('first')).toBeTruthy();
		expect(screen.getByText('second')).toBeTruthy();
	});

	it('starts a new group after 5 minutes from the same author', () => {
		renderTimeline([
			message({
				message_id: 'm-1',
				event_id: 'e-1',
				author: { display_name: 'Ada', avatar_url: null, is_current_user: false },
				event_created_at: '2026-08-12T10:00:00Z'
			}),
			message({
				message_id: 'm-2',
				event_id: 'e-2',
				author: { display_name: 'Ada', avatar_url: null, is_current_user: false },
				author_pubkey: 'pk-a',
				event_created_at: '2026-08-12T10:10:00Z'
			})
		]);
		expect(screen.getAllByText('Ada')).toHaveLength(2);
	});

	it('shows the empty state when no messages are present', () => {
		renderTimeline([]);
		expect(screen.getByText('No messages yet')).toBeTruthy();
	});
});

describe('MessageTimeline attachment affordance', () => {
	let clickSpy: ReturnType<typeof vi.spyOn>;
	let openSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		mocks.openChatAttachment.mockReset();
		clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
		openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);
		URL.createObjectURL = vi.fn(() => 'blob:test-attachment');
		URL.revokeObjectURL = vi.fn();
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('renders one affordance per attachment, in event order', () => {
		renderTimeline([
			message({
				message_id: 'm-2',
				attachments: [
					{
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: 'f-1',
						version: null
					},
					{
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: 'f-2',
						version: null
					}
				]
			})
		]);
		const chips = screen.getAllByRole('button', { name: /Open attachment/ });
		expect(chips).toHaveLength(2);
		expect(screen.getByText('f-1')).toBeTruthy();
		expect(screen.getByText('f-2')).toBeTruthy();
	});

	it('renders no affordance without attachments', () => {
		renderTimeline([message({ attachments: [] })]);
		expect(screen.queryByRole('button', { name: /Open attachment/ })).toBeNull();
	});

	it('treats refs differing only by version as distinct attachments', () => {
		renderTimeline([
			message({
				attachments: [
					{
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: 'f-1',
						version: null
					},
					{
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: 'f-1',
						version: 'sha256:0123abcdef'
					}
				]
			})
		]);
		expect(screen.getAllByRole('button', { name: /Open attachment/ })).toHaveLength(2);
	});

	it('pins the server wire shape end-to-end: camelCase attachment fields reach the open endpoint intact', async () => {
		mocks.openChatAttachment.mockResolvedValue(new Blob(['x']));
		// Raw server-shaped payload — exactly what
		// GET /applications/chat/messages emits today (ChatAttachmentDto
		// serializes camelCase, matching ResourceRef). If the server wire shape
		// and the frontend declarations ever diverge again, JSON.stringify of
		// the sent body loses fields and this test fails.
		const wire = JSON.parse(`{
			"message_id": "m-1",
			"event_id": "e-1",
			"community_id": "community-1",
			"channel_id": "general",
			"channel_kind": "topic",
			"author_pubkey": "pk-a",
			"event_created_at": "2026-08-12T10:00:00Z",
			"thread_root_id": null,
			"body": "with attachment",
			"attachments": [
				{"application": "io.elembra.files", "resourceType": "file", "resourceId": "f-1", "version": null}
			]
		}`) as ChatMessageDto;
		renderTimeline([wire]);

		await fireEvent.click(screen.getByRole('button', { name: 'Open attachment' }));
		await waitFor(() => expect(mocks.openChatAttachment).toHaveBeenCalledTimes(1));
		const sent = mocks.openChatAttachment.mock.calls[0][0];
		expect(JSON.stringify(sent)).toBe(
			'{"application":"io.elembra.files","resourceType":"file","resourceId":"f-1","version":null}'
		);
	});

	it('downloads the authorized bytes via an anchor click (no popup)', async () => {
		mocks.openChatAttachment.mockResolvedValue(new Blob(['plan content'], { type: 'text/plain' }));
		const attachment = {
			application: 'io.elembra.files',
			resourceType: 'file',
			resourceId: 'f-1',
			version: null
		};
		renderTimeline([message({ attachments: [attachment] })]);

		await fireEvent.click(screen.getByRole('button', { name: 'Open attachment' }));
		await waitFor(() => expect(mocks.openChatAttachment).toHaveBeenCalledTimes(1));
		expect(mocks.openChatAttachment).toHaveBeenCalledWith(attachment);
		await waitFor(() => expect(clickSpy).toHaveBeenCalledTimes(1));
		expect(URL.createObjectURL).toHaveBeenCalledWith(
			new Blob(['plan content'], { type: 'text/plain' })
		);
		expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:test-attachment');
		expect(openSpy).not.toHaveBeenCalled();
	});

	it('fails silently when the file is unauthorized or missing (existence-hiding)', async () => {
		mocks.openChatAttachment.mockRejectedValue(new Error('resource unavailable'));
		renderTimeline([
			message({
				attachments: [
					{
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: 'f-1',
						version: null
					}
				]
			})
		]);

		await fireEvent.click(screen.getByRole('button', { name: 'Open attachment' }));
		await waitFor(() => expect(mocks.openChatAttachment).toHaveBeenCalledTimes(1));
		expect(clickSpy).not.toHaveBeenCalled();
		// Nothing leaks into the DOM either.
		expect(screen.queryByText(/unavailable|error/i)).toBeNull();
	});
});

describe('MessageTimeline hover menu', () => {
	it('shows Ask and Copy link items when Ask is available', async () => {
		renderTimeline([message({ message_id: 'm-1', channel_id: 'general' })]);
		await fireEvent.click(screen.getByRole('button', { name: 'Message actions' }));
		expect(screen.getByRole('menuitem', { name: 'Ask Elembra about this' })).toBeTruthy();
		expect(screen.getByRole('menuitem', { name: 'Copy message link' })).toBeTruthy();
	});

	it(' Ask Elembra item is disabled when Ask is unavailable', async () => {
		const { rerender } = render(MessageTimeline, {
			props: {
				messages: [message({ message_id: 'm-1' })],
				loading: false,
				focusTarget: null,
				onLoadMore: vi.fn(),
				askAvailable: false,
				communityId: 'community-1'
			}
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Message actions' }));
		const item = screen.getByRole('menuitem', { name: 'Ask Elembra about this' });
		expect(item.className).toContain('cursor-not-allowed');
	});

	it('copies a deep link to the message when Copy message link is clicked', async () => {
		vi.mocked(mocks.copyToClipboard).mockReset();
		renderTimeline([message({ message_id: 'm-1', channel_id: 'general' })]);
		await fireEvent.click(screen.getByRole('button', { name: 'Message actions' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Copy message link' }));
		await waitFor(() =>
			expect(mocks.copyToClipboard).toHaveBeenCalledWith(
				expect.stringContaining('?channel=general&message=m-1')
			)
		);
	});
});
