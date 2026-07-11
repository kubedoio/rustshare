import { describe, it, expect, vi } from 'vitest';
import { mailApi } from './mail';
import { apiClient } from './client';

vi.mock('./client', () => ({
	apiClient: {
		get: vi.fn(),
		requestText: vi.fn(),
		getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
	}
}));

describe('mailApi', () => {
	it('lists messages', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ messages: [{ id: '1', subject: 'Hi' }] });
		const result = await mailApi.listMessages();
		expect(result).toHaveLength(1);
		expect(apiClient.get).toHaveBeenCalledWith('/mail/messages');
	});

	it('fetches part content as text', async () => {
		vi.mocked(apiClient.requestText).mockResolvedValueOnce('hello');
		const result = await mailApi.getPartContent('msg-1', 'part-1');
		expect(result).toBe('hello');
		expect(apiClient.requestText).toHaveBeenCalledWith('/mail/messages/msg-1/parts/part-1');
	});
});
