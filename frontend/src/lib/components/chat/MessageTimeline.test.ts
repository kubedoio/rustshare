import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import MessageTimeline from './MessageTimeline.svelte';
import type { ChatMessageDto } from '$lib/api/chat';

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
		...overrides
	};
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
		render(MessageTimeline, {
			props: {
				messages: [target],
				loading: false,
				focusTarget: target,
				onLoadMore: vi.fn()
			}
		});
		expect(screen.getByText('hello')).toBeTruthy();
		expect(scrollSpy).toHaveBeenCalledTimes(1);
	});

	it('scrolls once the focused message arrives after the initial page load', async () => {
		const { rerender } = render(MessageTimeline, {
			props: {
				messages: [],
				loading: false,
				focusTarget: null,
				onLoadMore: vi.fn()
			}
		});
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
