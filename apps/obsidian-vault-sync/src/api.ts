// Disclaimer: Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with, endorsed by, or sponsored by Obsidian.
// RustShare Vault Sync — API client for the RustShare vault sync backend.

import { requestUrl, RequestUrlResponse } from 'obsidian';

export interface Vault {
  id: string;
  name: string;
  adapter: 'ObsidianVault';
  root_path?: string;
  server_rev: number;
  created_at: string;
  updated_at: string;
}

export interface VaultManifest {
  vault_id: string;
  adapter: 'ObsidianVault';
  server_rev: number;
  generated_at: string;
  files: VaultManifestEntry[];
}

export interface VaultManifestEntry {
  path: string;
  sha256?: string;
  size?: number;
  content_type?: string;
  server_rev: number;
  mtime_server: string;
  deleted: boolean;
  deleted_at?: string | null;
}

export interface CreateVaultRequest {
  name: string;
  adapter: 'ObsidianVault';
  client_vault_id?: string;
  device_id: string;
}

export interface UploadFileHeaders {
  'X-RustShare-Base-Server-Rev': string;
  'X-RustShare-SHA256': string;
  'X-RustShare-Device-ID': string;
}

export interface DeleteFileHeaders {
  'X-RustShare-Base-Server-Rev': string;
  'X-RustShare-Device-ID': string;
}

export interface RenameRequest {
  old_path: string;
  new_path: string;
  base_server_rev: number;
  device_id: string;
}

export interface ConflictError {
  error: 'conflict';
  message: string;
  path: string;
  client_rev: number;
  current_rev: number;
  server_sha256?: string;
  resolution: 'create_conflict_copy';
}

export interface DeviceRequestResponse {
  user_code: string;
  device_code: string;
  expires_in: number;
  verification_uri: string;
  verification_uri_complete: string;
}

export type DevicePollResponse =
  | { status: 'pending' }
  | { status: 'approved'; token: string }
  | { status: 'expired' };

export class RustShareAPI {
  constructor(private baseUrl: string, private authToken: string) {
    this.validateBaseUrl(baseUrl);
  }

  private validateBaseUrl(url: string): void {
    try {
      const parsed = new URL(url);
      const allowed = parsed.protocol === 'https:' ||
        (parsed.protocol === 'http:' && /^localhost(:\d+)?$/.test(parsed.hostname));
      if (!allowed) {
        throw new Error(`Invalid URL scheme: ${parsed.protocol}. Only HTTPS is allowed.`);
      }
      if (parsed.username || parsed.password) {
        throw new Error('URL must not contain credentials');
      }
    } catch (e) {
      if (e instanceof TypeError) {
        throw new Error(`Invalid URL: ${url}`);
      }
      throw e;
    }
  }

  private buildUrl(endpoint: string): string {
    return `${this.baseUrl.replace(/\/$/, '')}/api/vault-sync/v1${endpoint}`;
  }

  private buildAuthUrl(endpoint: string): string {
    return `${this.baseUrl.replace(/\/$/, '')}/api/v1${endpoint}`;
  }

  private encodePath(path: string): string {
    return path.replace(/\/+/g, '/').split('/').map(encodeURIComponent).join('/');
  }

  private getMimeType(path: string): string {
    const ext = path.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'md': return 'text/markdown';
      case 'txt': return 'text/plain';
      case 'png': return 'image/png';
      case 'jpg':
      case 'jpeg': return 'image/jpeg';
      case 'gif': return 'image/gif';
      case 'svg': return 'image/svg+xml';
      case 'pdf': return 'application/pdf';
      case 'json': return 'application/json';
      case 'js': return 'application/javascript';
      case 'css': return 'text/css';
      case 'html':
      case 'htm': return 'text/html';
      default: return 'application/octet-stream';
    }
  }

  private requestUrlWithTimeout(
    url: string,
    init: { method: string; headers: Record<string, string>; body?: string | ArrayBuffer },
    timeoutMs = 30000
  ): Promise<RequestUrlResponse> {
    return Promise.race([
      requestUrl({ url, method: init.method, headers: init.headers, body: init.body }),
      new Promise<RequestUrlResponse>((_, reject) => {
        setTimeout(() => reject(new Error('Request timeout')), timeoutMs);
      }),
    ]);
  }

  private async request<T>(method: string, endpoint: string, body?: unknown, extraHeaders?: Record<string, string>, urlBuilder?: (endpoint: string) => string): Promise<T>;
  private async request(method: string, endpoint: string, body?: unknown, extraHeaders?: Record<string, string>, urlBuilder?: (endpoint: string) => string): Promise<unknown> {
    const headers: Record<string, string> = {
      'Accept': 'application/json, */*;q=0.8',
      'User-Agent': 'RustShare-Obsidian-VaultSync/0.1.0',
    };
    if (this.authToken) {
      headers['Authorization'] = `Bearer ${this.authToken}`;
    }
    if (!['GET', 'HEAD', 'OPTIONS', 'TRACE'].includes(method)) {
      headers['X-Rustshare-Csrf'] = '1';
    }
    Object.assign(headers, extraHeaders);

    if (body !== undefined && !(body instanceof ArrayBuffer) && !headers['Content-Type']) {
      headers['Content-Type'] = 'application/json';
    }

    const url = urlBuilder ? urlBuilder(endpoint) : this.buildUrl(endpoint);
    const requestBody = body instanceof ArrayBuffer ? body : body !== undefined ? JSON.stringify(body) : undefined;

    console.log('RustShare API request', { url, method, headers: { ...headers, Authorization: headers.Authorization ? '***' : undefined }, bodyLength: typeof requestBody === 'string' ? requestBody.length : requestBody?.byteLength });
    const response = await this.requestUrlWithTimeout(url, { method, headers, body: requestBody });
    console.log('RustShare API response', { url, status: response.status, bodyPreview: response.text.slice(0, 500) });

    if (response.status === 409) {
      const conflict = (response.json ?? {}) as Partial<ConflictError>;
      const error: ConflictError = {
        error: 'conflict',
        message: conflict.message ?? 'Conflict detected',
        path: conflict.path ?? endpoint,
        client_rev: conflict.client_rev ?? 0,
        current_rev: conflict.current_rev ?? 0,
        server_sha256: conflict.server_sha256,
        resolution: conflict.resolution ?? 'create_conflict_copy',
      };
      const err = new Error(error.message || 'Conflict');
      Object.assign(err, error);
      throw err;
    }

    if (response.status === 429) {
      const retryAfter = response.headers['retry-after'] || response.headers['Retry-After'];
      let retryAfterSeconds: number | undefined;
      if (retryAfter) {
        const trimmed = retryAfter.trim();
        // Only parse if it's a pure integer (delta-seconds). Ignore HTTP-date strings.
        if (/^\d+$/.test(trimmed)) {
          retryAfterSeconds = parseInt(trimmed, 10);
        }
      }
      const err = new Error(`HTTP 429: ${response.text}`);
      (err as any).status = 429;
      (err as any).retry_after = retryAfterSeconds;
      throw err;
    }

    if (response.status < 200 || response.status >= 300) {
      throw new Error(`HTTP ${response.status}: ${response.text}`);
    }

    if (response.status === 204) {
      return undefined;
    }

    const contentType = response.headers['content-type'] ?? response.headers['Content-Type'] ?? '';
    if (contentType.includes('application/json')) {
      return response.json;
    }

    return response.arrayBuffer;
  }

  // Device pairing methods
  async requestDevicePairing(): Promise<DeviceRequestResponse> {
    return this.request<DeviceRequestResponse>('POST', '/auth/device/request', undefined, undefined, this.buildAuthUrl.bind(this));
  }

  async pollDevicePairing(deviceCode: string): Promise<DevicePollResponse> {
    return this.request<DevicePollResponse>('POST', '/auth/device/poll', { device_code: deviceCode }, undefined, this.buildAuthUrl.bind(this));
  }

  // Vault methods
  async createVault(req: CreateVaultRequest): Promise<Vault> {
    return this.request<Vault>('POST', '/vaults', req);
  }

  async listVaults(): Promise<{ vaults: Vault[] }> {
    return this.request<{ vaults: Vault[] }>('GET', '/vaults');
  }

  async getVault(vaultId: string): Promise<Vault> {
    return this.request<Vault>('GET', `/vaults/${encodeURIComponent(vaultId)}`);
  }

  async getManifest(vaultId: string): Promise<VaultManifest> {
    return this.request<VaultManifest>('GET', `/vaults/${encodeURIComponent(vaultId)}/manifest`);
  }

  // File methods
  async uploadFile(vaultId: string, path: string, content: ArrayBuffer, sha256: string, baseServerRev: number, deviceId: string): Promise<{ server_rev: number }> {
    return this.request<{ server_rev: number }>('PUT', `/vaults/${encodeURIComponent(vaultId)}/files/${this.encodePath(path)}`, content, {
      'Content-Type': this.getMimeType(path),
      'X-RustShare-Base-Server-Rev': String(baseServerRev),
      'X-RustShare-SHA256': sha256,
      'X-RustShare-Device-ID': deviceId,
    });
  }

  async downloadFile(vaultId: string, path: string): Promise<ArrayBuffer> {
    return this.request<ArrayBuffer>('GET', `/vaults/${encodeURIComponent(vaultId)}/files/${this.encodePath(path)}`);
  }

  async deleteFile(vaultId: string, path: string, baseServerRev: number, deviceId: string): Promise<void> {
    await this.request<void>('DELETE', `/vaults/${encodeURIComponent(vaultId)}/files/${this.encodePath(path)}`, undefined, {
      'X-RustShare-Base-Server-Rev': String(baseServerRev),
      'X-RustShare-Device-ID': deviceId,
    });
  }

  async renameFile(vaultId: string, req: RenameRequest): Promise<void> {
    const { old_path, new_path, base_server_rev, device_id } = req;
    await this.request<void>('POST', `/vaults/${encodeURIComponent(vaultId)}/rename`, { old_path, new_path }, {
      'X-RustShare-Base-Server-Rev': String(base_server_rev),
      'X-RustShare-Device-ID': device_id,
    });
  }

  // Device methods
  async registerDevice(deviceName: string, clientType: string, clientVersion?: string): Promise<{ id: string }> {
    return this.request<{ id: string }>('POST', '/devices/register', {
      device_name: deviceName,
      client_type: clientType,
      client_version: clientVersion,
    });
  }

  async revokeDevice(deviceId: string): Promise<void> {
    await this.request<void>('DELETE', `/devices/${encodeURIComponent(deviceId)}`);
  }
}
