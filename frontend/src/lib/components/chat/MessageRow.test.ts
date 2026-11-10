import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import MessageRow from './MessageRow.svelte';
import type { ChatMessageDto } from '$lib/api/chat';

vi.mock('$lib/api/chat', () => ({
	openChatAttachment: vi.fn()
}));

vi.mock('$lib/api/client', () => ({
	apiClient: { getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1') }
}));

function message(overrides: Partial<ChatMessageDto> = {}): ChatMessageDto {
	return {
		message_id: 'm-1',
		event_id: 'e1'.repeat(32),
		community_id: 'community-1',
		channel_id: '11111111-2222-4333-8444-555555555555',
		channel_kind: 'stream',
		author_pubkey: 'a1'.repeat(32),
		author: { display_name: 'Alice', avatar_url: null, is_current_user: false },
		event_created_at: '2026-11-09T10:00:00Z',
		thread_root_id: null,
		body: 'hello channel',
		attachments: [],
		...overrides
	};
}

function renderRow(props: Record<string, unknown> = {}) {
	return render(MessageRow, {
		message: message(),
		showHeader: true,
		askAvailable: false,
		...props
	});
}

describe('MessageRow conversation affordances', () => {
	it('renders deleted state instead of the body and hides actions', () => {
		renderRow({ deletedInfo: { actor: 'a1'.repeat(32), via: 'nip09' } });
		expect(screen.getByText('Message deleted.')).toBeTruthy();
		expect(screen.queryByText('hello channel')).toBeNull();
		expect(screen.queryByRole('button', { name: 'Message actions' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'Reply to message' })).toBeNull();
	});

	it('renders reaction chips with counts and pressed state for the viewer’s own', () => {
		const onReact = vi.fn();
		renderRow({
			reactionsAvailable: true,
			reactions: [
				{
					targetId: 'e1'.repeat(32),
					emoji: '👍',
					count: 2,
					authors: ['a1'.repeat(32), 'b2'.repeat(32)],
					myEventId: 'r1'.repeat(32)
				},
				{
					targetId: 'e1'.repeat(32),
					emoji: '🎉',
					count: 1,
					authors: ['c3'.repeat(32)],
					myEventId: null
				}
			],
			onReact
		});
		const mine = screen.getByRole('button', { name: 'Reaction 👍, 2' });
		expect(mine.getAttribute('aria-pressed')).toBe('true');
		expect(
			screen.getByRole('button', { name: 'Reaction 🎉, 1' }).getAttribute('aria-pressed')
		).toBe('false');
	});

	it('toggles a reaction and offers the palette through the smiley menu', async () => {
		const onReact = vi.fn();
		renderRow({ reactionsAvailable: true, onReact });
		await fireEvent.click(screen.getByRole('button', { name: 'Add reaction' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'React 🎉' }));
		expect(onReact).toHaveBeenCalledWith('🎉');
	});

	it('offers delete only with a relay-backed mode', async () => {
		const onDelete = vi.fn();
		renderRow({ deleteMode: 'self', onDelete });
		await fireEvent.click(screen.getByRole('button', { name: 'Message actions' }));
		await fireEvent.click(screen.getByRole('menuitem', { name: 'Delete message' }));
		expect(onDelete).toHaveBeenCalledTimes(1);
	});

	it('hides the delete entry without a mode and the thread buttons without the capability', () => {
		renderRow({ deleteMode: null, threadAvailable: false });
		expect(screen.queryByRole('button', { name: 'Reply to message' })).toBeNull();
		expect(screen.queryByRole('button', { name: 'View thread, 3 replies' })).toBeNull();
	});

	it('shows the thread button with the observed reply count and emits open/reply', async () => {
		const onOpenThread = vi.fn();
		const onReply = vi.fn();
		renderRow({ threadAvailable: true, replyCount: 3, onOpenThread, onReply });
		await fireEvent.click(screen.getByRole('button', { name: 'View thread, 3 replies' }));
		expect(onOpenThread).toHaveBeenCalledTimes(1);
		await fireEvent.click(screen.getByRole('button', { name: 'Reply to message' }));
		expect(onReply).toHaveBeenCalledTimes(1);
	});
});
