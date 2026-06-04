import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiClient } from './client';

function jsonResponse(body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status: 200,
		headers: { 'Content-Type': 'application/json' }
	});
}

describe('ApiClient URL construction', () => {
	beforeEach(() => {
		vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse({ ok: true })));
	});

	it('prefixes relative endpoints with a relative API base URL', async () => {
		const client = new ApiClient('/api/v1');

		await client.get('/files');

		expect(fetch).toHaveBeenCalledWith(
			'/api/v1/files',
			expect.objectContaining({ method: 'GET' })
		);
	});

	it('does not duplicate a relative API base URL for already-prefixed endpoints', async () => {
		const client = new ApiClient('/api/v1');

		await client.get('/api/v1/files/file-123/content');

		expect(fetch).toHaveBeenCalledWith(
			'/api/v1/files/file-123/content',
			expect.objectContaining({ method: 'GET' })
		);
	});

	it('does not duplicate an absolute API base URL for already-prefixed endpoints', async () => {
		const client = new ApiClient('http://localhost:8080/api/v1');

		await client.get('/api/v1/files/file-123/content');

		expect(fetch).toHaveBeenCalledWith(
			'http://localhost:8080/api/v1/files/file-123/content',
			expect.objectContaining({ method: 'GET' })
		);
	});

	it('passes absolute endpoint URLs through unchanged', async () => {
		const client = new ApiClient('/api/v1');

		await client.get('https://storage.example/files/file-123');

		expect(fetch).toHaveBeenCalledWith(
			'https://storage.example/files/file-123',
			expect.objectContaining({ method: 'GET' })
		);
	});
});
