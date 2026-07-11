import { beforeEach, describe, it, expect, vi } from 'vitest';
import { mailApi } from './mail';
import { apiClient } from './client';

vi.mock('./client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn(),
		patch: vi.fn(),
		delete: vi.fn(),
		requestText: vi.fn(),
		getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
	}
}));

describe('mailApi', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

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

	it('creates accounts without reading passwords back', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({
			id: 'acct-1',
			name: 'Work',
			username: 'me@example.com'
		});

		await mailApi.createAccount({
			name: 'Work',
			host: 'imap.example.com',
			port: 993,
			username: 'me@example.com',
			password: 'secret',
			tls_mode: 'tls'
		});

		expect(apiClient.post).toHaveBeenCalledWith('/mail/accounts', {
			name: 'Work',
			host: 'imap.example.com',
			port: 993,
			username: 'me@example.com',
			password: 'secret',
			tls_mode: 'tls'
		});
	});

	it('loads a bounded folder message page', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ uidvalidity: 42, messages: [] });

		await mailApi.listAccountMessages('acct-1', 'INBOX/Team', 100);

		expect(apiClient.get).toHaveBeenCalledWith(
			'/mail/accounts/acct-1/messages?folder=INBOX%2FTeam&limit=100'
		);
	});

	it('queues import and archive jobs', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({ id: 'import-1' });
		vi.mocked(apiClient.post).mockResolvedValueOnce({ id: 'archive-1' });

		await mailApi.createImportJob('acct-1', {
			folder_name: 'INBOX',
			source_uidvalidity: 42,
			selected_uids: [1, 2]
		});
		await mailApi.createArchiveJob('acct-1', {
			folder_name: 'INBOX',
			retention_days: 365,
			max_retries: 5
		});

		expect(apiClient.post).toHaveBeenNthCalledWith(1, '/mail/accounts/acct-1/import', {
			folder_name: 'INBOX',
			source_uidvalidity: 42,
			selected_uids: [1, 2]
		});
		expect(apiClient.post).toHaveBeenNthCalledWith(2, '/mail/accounts/acct-1/archive-jobs', {
			folder_name: 'INBOX',
			retention_days: 365,
			max_retries: 5
		});
	});

	it('refreshes import job status', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ id: 'import-1', status: 'completed' });

		const job = await mailApi.getImportJob('import-1');

		expect(job.status).toBe('completed');
		expect(apiClient.get).toHaveBeenCalledWith('/mail/import-jobs/import-1');
	});

	it('manages mail links', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ links: [{ id: 'link-1' }] });
		vi.mocked(apiClient.post).mockResolvedValueOnce({ id: 'link-2' });
		vi.mocked(apiClient.delete).mockResolvedValueOnce(undefined);

		const links = await mailApi.listLinks('msg-1');
		await mailApi.createLink('msg-1', { target_type: 'file', target_id: 'file-1' });
		await mailApi.deleteLink('msg-1', 'link-1');

		expect(links).toHaveLength(1);
		expect(apiClient.get).toHaveBeenCalledWith('/mail/messages/msg-1/links');
		expect(apiClient.post).toHaveBeenCalledWith('/mail/messages/msg-1/links', {
			target_type: 'file',
			target_id: 'file-1'
		});
		expect(apiClient.delete).toHaveBeenCalledWith('/mail/messages/msg-1/links/link-1');
	});
});
