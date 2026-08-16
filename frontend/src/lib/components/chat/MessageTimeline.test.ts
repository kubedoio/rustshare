import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import MessageTimeline from './MessageTimeline.svelte';
import type { ChatMessageDto } from '$lib/api/chat';

const mocks = vi.hoisted(() => ({
	openChatAttachment: vi.fn()
}));

vi.mock('$lib/api/chat', () => ({
	openChatAttachment: mocks.openChatAttachment
}));

function message(overrides: Partial<ChatMessageDto> = {}): ChatMessageDto {
	return {
		message_id: 'm-1',
		event_id: 'e-1',
		community_id: 'community-1',
		channel_id: 'general',
		channel_kind: 'topic',
		author_pubkey: 'pk-a',
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
			onLoadMore: vi.fn()
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
});

describe('MessageTimeline attachment affordance', () => {
	beforeEach(() => {
		mocks.openChatAttachment.mockReset();
		URL.createObjectURL = vi.fn(() => 'blob:test-attachment');
		URL.revokeObjectURL = vi.fn();
		vi.stubGlobal('open', vi.fn());
	});

	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
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
		const chips = screen.getAllByRole('button', { name: /Attachment/ });
		expect(chips).toHaveLength(2);
		expect(screen.getByText('Attachment 1')).toBeTruthy();
		expect(screen.getByText('Attachment 2')).toBeTruthy();
	});

	it('renders no affordance without attachments', () => {
		renderTimeline([message({ attachments: [] })]);
		expect(screen.queryByRole('button', { name: /Attachment/ })).toBeNull();
	});

	it('opens through the Files authorization path and shows the bytes in a new tab', async () => {
		mocks.openChatAttachment.mockResolvedValue(new Blob(['plan content'], { type: 'text/plain' }));
		const attachment = {
			application: 'io.elembra.files',
			resourceType: 'file',
			resourceId: 'f-1',
			version: null
		};
		renderTimeline([message({ attachments: [attachment] })]);

		await fireEvent.click(screen.getByRole('button', { name: 'Attachment' }));
		await waitFor(() => expect(mocks.openChatAttachment).toHaveBeenCalledTimes(1));
		expect(mocks.openChatAttachment).toHaveBeenCalledWith(attachment);
		await waitFor(() => expect(window.open).toHaveBeenCalledWith('blob:test-attachment', '_blank'));
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

		await fireEvent.click(screen.getByRole('button', { name: 'Attachment' }));
		await waitFor(() => expect(mocks.openChatAttachment).toHaveBeenCalledTimes(1));
		expect(window.open).not.toHaveBeenCalled();
		// Nothing leaks into the DOM either.
		expect(screen.queryByText(/unavailable|error/i)).toBeNull();
	});
});
