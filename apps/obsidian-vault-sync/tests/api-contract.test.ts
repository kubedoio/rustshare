import { describe, it, expect, beforeEach, vi } from 'vitest';
import { RustShareAPI, ConflictError } from '../src/api';

function createMockResponse(options: {
  status: number;
  jsonData?: unknown;
  contentType?: string;
}): Response {
  const { status, jsonData, contentType = 'application/json' } = options;
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 409 ? 'Conflict' : 'OK',
    headers: {
      get: (name: string) => {
        if (name.toLowerCase() === 'content-type') return contentType;
        return null;
      },
    } as unknown as Headers,
    json: async () => jsonData,
    text: async () => (jsonData ? JSON.stringify(jsonData) : ''),
    arrayBuffer: async () => new ArrayBuffer(0),
    clone: () => createMockResponse(options),
    body: null,
    bodyUsed: false,
    redirected: false,
    type: 'basic',
    url: '',
  } as Response;
}

describe('API contract verification', () => {
  let api: RustShareAPI;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    api = new RustShareAPI('https://api.rustshare.test', 'test-token');
    fetchMock = vi.fn();
    global.fetch = fetchMock;
  });

  describe('ConflictError parsing', () => {
    it('parses 409 response into ConflictError with correct fields', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({
          status: 409,
          jsonData: {
            error: 'Conflict',
            client_rev: 5,
            current_rev: 7,
            server_sha256: 'abc123',
          },
        })
      );

      try {
        await api.uploadFile(
          'vault-1',
          'notes/test.md',
          new ArrayBuffer(0),
          'hash',
          1,
          'device-1'
        );
        expect.fail('expected throw');
      } catch (e: any) {
        expect(e).toBeInstanceOf(Error);
        expect(e.message).toBe('Conflict detected');
        expect(e.error).toBe('conflict');
        expect(e.path).toBe('/vaults/vault-1/files/notes/test.md');
        expect(e.client_rev).toBe(5);
        expect(e.current_rev).toBe(7);
        expect(e.server_sha256).toBe('abc123');
        expect(e.resolution).toBe('create_conflict_copy');
      }
    });

    it('preserves all fields from a full 409 response', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({
          status: 409,
          jsonData: {
            error: 'Conflict',
            message: 'File was modified',
            path: 'notes/x.md',
            client_rev: 5,
            current_rev: 7,
            server_sha256: 'abc123',
            resolution: 'download_server_version',
          },
        })
      );

      try {
        await api.uploadFile(
          'vault-1',
          'notes/x.md',
          new ArrayBuffer(0),
          'hash',
          1,
          'device-1'
        );
        expect.fail('expected throw');
      } catch (e: any) {
        expect(e).toBeInstanceOf(Error);
        expect(e.message).toBe('File was modified');
        expect(e.error).toBe('conflict');
        expect(e.path).toBe('notes/x.md');
        expect(e.client_rev).toBe(5);
        expect(e.current_rev).toBe(7);
        expect(e.server_sha256).toBe('abc123');
        expect(e.resolution).toBe('download_server_version');
      }
    });
  });

  describe('Header names', () => {
    it('uploadFile sends exact backend header names', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.uploadFile(
        'vault-1',
        'notes/test.md',
        new ArrayBuffer(4),
        'sha256-abc',
        3,
        'device-123'
      );

      const [, init] = fetchMock.mock.calls[0];
      const headers = init.headers as Record<string, string>;

      expect(headers).toHaveProperty('X-RustShare-SHA256', 'sha256-abc');
      expect(headers).toHaveProperty('X-RustShare-Base-Server-Rev', '3');
      expect(headers).toHaveProperty('X-RustShare-Device-ID', 'device-123');
      expect(headers).toHaveProperty('Content-Type', 'text/markdown');
    });

    it('deleteFile sends exact backend header names', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.deleteFile('vault-1', 'notes/test.md', 2, 'device-123');

      const [, init] = fetchMock.mock.calls[0];
      const headers = init.headers as Record<string, string>;

      expect(headers).toHaveProperty('X-RustShare-Base-Server-Rev', '2');
      expect(headers).toHaveProperty('X-RustShare-Device-ID', 'device-123');
    });

    it('includes Authorization and Content-Type for JSON bodies', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({ status: 200, jsonData: { id: 'd1' } })
      );

      const result = await api.registerDevice('my-device', 'obsidian');

      const [, init] = fetchMock.mock.calls[0];
      const headers = init.headers as Record<string, string>;

      expect(headers).toHaveProperty('Authorization', 'Bearer test-token');
      expect(headers).toHaveProperty('Content-Type', 'application/json');
      expect(result.id).toBe('d1');
    });

    it('includes CSRF header on mutating methods', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.uploadFile('vault-1', 'test.md', new ArrayBuffer(0), 'hash', 1, 'device-1');
      const [, init1] = fetchMock.mock.calls[0];
      expect(init1.headers).toHaveProperty('X-Rustshare-Csrf', '1');

      fetchMock.mockClear();
      fetchMock.mockResolvedValue(createMockResponse({ status: 200, jsonData: { id: 'd1' } }));
      await api.registerDevice('my-device', 'obsidian');
      const [, init2] = fetchMock.mock.calls[0];
      expect(init2.headers).toHaveProperty('X-Rustshare-Csrf', '1');

      fetchMock.mockClear();
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));
      await api.deleteFile('vault-1', 'test.md', 1, 'device-1');
      const [, init3] = fetchMock.mock.calls[0];
      expect(init3.headers).toHaveProperty('X-Rustshare-Csrf', '1');

      fetchMock.mockClear();
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));
      await api.renameFile('vault-1', { old_path: 'a.md', new_path: 'b.md', base_server_rev: 1, device_id: 'device-1' });
      const [, init4] = fetchMock.mock.calls[0];
      expect(init4.headers).toHaveProperty('X-Rustshare-Csrf', '1');
    });
  });

  describe('URL path construction', () => {
    it('builds vault-sync file URLs correctly', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.uploadFile(
        'vault-1',
        'notes/hello.md',
        new ArrayBuffer(0),
        'hash',
        1,
        'device-1'
      );

      const [url] = fetchMock.mock.calls[0];
      expect(url).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault-1/files/notes/hello.md'
      );
    });

    it('builds manifest URL correctly', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({
          status: 200,
          jsonData: { vault_id: 'v1', adapter: 'obsidian_vault', server_rev: 1, generated_at: new Date().toISOString(), files: [] },
        })
      );

      await api.getManifest('vault-1');

      const [url] = fetchMock.mock.calls[0];
      expect(url).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault-1/manifest'
      );
    });

    it('builds device register URL correctly', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({ status: 200, jsonData: { id: 'd1' } })
      );

      await api.registerDevice('my-device', 'obsidian');

      const [url] = fetchMock.mock.calls[0];
      expect(url).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/devices/register'
      );
    });

    it('URL-encodes vault IDs and file paths', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.uploadFile(
        'vault test',
        'notes/hello world.md',
        new ArrayBuffer(0),
        'hash',
        1,
        'device-1'
      );

      const [url] = fetchMock.mock.calls[0];
      expect(url).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault%20test/files/notes/hello%20world.md'
      );
    });

    it('URL-encodes special characters ? # & in file paths', async () => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await api.uploadFile('vault-1', 'notes/what?.md', new ArrayBuffer(0), 'hash', 1, 'device-1');
      expect(fetchMock.mock.calls[0][0]).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault-1/files/notes/what%3F.md'
      );

      fetchMock.mockClear();
      await api.uploadFile('vault-1', 'notes/hash#1.md', new ArrayBuffer(0), 'hash', 1, 'device-1');
      expect(fetchMock.mock.calls[0][0]).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault-1/files/notes/hash%231.md'
      );

      fetchMock.mockClear();
      await api.uploadFile('vault-1', 'notes/a&b.md', new ArrayBuffer(0), 'hash', 1, 'device-1');
      expect(fetchMock.mock.calls[0][0]).toBe(
        'https://api.rustshare.test/api/vault-sync/v1/vaults/vault-1/files/notes/a%26b.md'
      );
    });
  });

  describe('uploadFile return value', () => {
    it('returns server_rev from backend', async () => {
      fetchMock.mockResolvedValue(
        createMockResponse({ status: 200, jsonData: { server_rev: 3 } })
      );

      const result = await api.uploadFile(
        'vault-1',
        'notes/test.md',
        new ArrayBuffer(4),
        'sha256-abc',
        2,
        'device-123'
      );

      expect(result).toEqual({ server_rev: 3 });
    });
  });

  describe('HTTP method mapping', () => {
    it.each([
      {
        method: 'uploadFile',
        args: [
          'vault-1',
          'test.md',
          new ArrayBuffer(0),
          'hash',
          1,
          'device-1',
        ],
        expected: 'PUT',
      },
      {
        method: 'downloadFile',
        args: ['vault-1', 'test.md'],
        expected: 'GET',
      },
      {
        method: 'deleteFile',
        args: ['vault-1', 'test.md', 1, 'device-1'],
        expected: 'DELETE',
      },
      {
        method: 'renameFile',
        args: [
          'vault-1',
          {
            old_path: 'a.md',
            new_path: 'b.md',
            base_server_rev: 1,
            device_id: 'device-1',
          },
        ],
        expected: 'POST',
      },
      {
        method: 'getManifest',
        args: ['vault-1'],
        expected: 'GET',
      },
      {
        method: 'registerDevice',
        args: ['my-device', 'obsidian'],
        expected: 'POST',
      },
      {
        method: 'revokeDevice',
        args: ['device-1'],
        expected: 'DELETE',
      },
    ])('$method uses $expected', async ({ method, args, expected }) => {
      fetchMock.mockResolvedValue(createMockResponse({ status: 204 }));

      await (api as any)[method](...args);

      const [, init] = fetchMock.mock.calls[0];
      expect(init.method).toBe(expected);
    });
  });
});
