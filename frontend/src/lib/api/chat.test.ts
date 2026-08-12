import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import {
	getChatMessages,
	getChatMessage,
	chatMessageRef,
	chatMessageIdFromRef
} from './chat';

vi.mock('./client', () => ({
	apiClient: { get: vi.fn() }
}));

describe('Chat API contract', () => {
	beforeEach(() => vi.mocked(apiClient.get).mockReset());

	describe('getChatMessages', () => {
		it('calls the messages endpoint with the channel id', async () => {
			vi.mocked(apiClient.get).mockResolvedValue({ messages: [], next_before: null });
			await getChatMessages('general');
			expect(apiClient.get).toHaveBeenCalledWith('/applications/chat/messages?channel_id=general');
		});

		it('URL-encodes the before watermark when present', async () => {
			vi.mocked(apiClient.get).mockResolvedValue({ messages: [], next_before: null });
			await getChatMessages('general', '2026-08-12T10:00:00Z');
			const endpoint = vi.mocked(apiClient.get).mock.calls[0][0] as string;
			expect(endpoint).toContain('before=2026-08-12T10%3A00%3A00Z');
		});
	});

	describe('getChatMessage', () => {
		it('calls the single-message endpoint', async () => {
			vi.mocked(apiClient.get).mockResolvedValue({ message_id: 'abc', body: null });
			await getChatMessage('abc');
			expect(apiClient.get).toHaveBeenCalledWith('/applications/chat/messages/abc');
		});
	});

	describe('chatMessageRef helpers', () => {
		it('builds and parses the canonical ref', () => {
			const ref = chatMessageRef('deadbeef');
			expect(ref).toBe('elembra://io.elembra.chat/message/deadbeef');
			expect(chatMessageIdFromRef(ref)).toBe('deadbeef');
		});

		it('returns null for non-chat refs', () => {
			expect(chatMessageIdFromRef('elembra://io.elembra.files/file/x')).toBeNull();
		});
	});
});
