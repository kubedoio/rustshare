import { ApiClient } from './client';
import { ApiError } from './types';
import type {
	Vault,
	VaultManifest,
	VaultDevice,
	CreateVaultRequest,
	RenameVaultFileRequest
} from './types';

const VAULT_SYNC_BASE_URL =
	(import.meta.env.VITE_API_URL || 'http://localhost:8080/api/v1').replace(/\/api\/v1$/, '') +
	'/api/vault-sync/v1';
const vaultSyncClient = new ApiClient(VAULT_SYNC_BASE_URL);

export async function createVault(req: CreateVaultRequest): Promise<Vault> {
	return vaultSyncClient.post<Vault>('/vaults', req);
}

export async function listVaults(): Promise<{ vaults: Vault[] }> {
	return vaultSyncClient.get<{ vaults: Vault[] }>('/vaults');
}

export async function getVault(vaultId: string): Promise<Vault> {
	return vaultSyncClient.get<Vault>(`/vaults/${vaultId}`);
}

export async function getManifest(vaultId: string): Promise<VaultManifest> {
	return vaultSyncClient.get<VaultManifest>(`/vaults/${vaultId}/manifest`);
}

export async function downloadVaultFile(vaultId: string, path: string): Promise<Blob> {
	const url = new URL(`/vaults/${vaultId}/files/${encodeURIComponent(path)}`, VAULT_SYNC_BASE_URL);
	const response = await fetch(url.toString(), {
		headers: {
			Authorization: `Bearer ${sessionStorage.getItem('rustshare.websocket_token') || ''}`
		},
		credentials: 'include'
	});
	if (!response.ok) throw new ApiError(response.status, `Download failed: ${response.status}`);
	return response.blob();
}

export async function uploadVaultFile(
	vaultId: string,
	path: string,
	content: Blob,
	sha256: string,
	baseServerRev: number,
	deviceId: string
): Promise<void> {
	const url = new URL(`/vaults/${vaultId}/files/${encodeURIComponent(path)}`, VAULT_SYNC_BASE_URL);
	const response = await fetch(url.toString(), {
		method: 'PUT',
		headers: {
			Authorization: `Bearer ${sessionStorage.getItem('rustshare.websocket_token') || ''}`,
			'Content-Type': 'application/octet-stream',
			'X-RustShare-Base-Server-Rev': String(baseServerRev),
			'X-RustShare-SHA256': sha256,
			'X-RustShare-Device-ID': deviceId,
			'X-Rustshare-Csrf': '1'
		},
		body: content,
		credentials: 'include'
	});
	if (!response.ok) throw new ApiError(response.status, `Upload failed: ${response.status}`);
}

export async function deleteVaultFile(
	vaultId: string,
	path: string,
	baseServerRev: number,
	deviceId: string
): Promise<void> {
	return vaultSyncClient.requestVoid(`/vaults/${vaultId}/files/${encodeURIComponent(path)}`, {
		method: 'DELETE',
		headers: {
			'X-RustShare-Base-Server-Rev': String(baseServerRev),
			'X-RustShare-Device-ID': deviceId
		}
	});
}

export async function renameVaultFile(vaultId: string, req: RenameVaultFileRequest): Promise<void> {
	return vaultSyncClient.postVoid(`/vaults/${vaultId}/rename`, req);
}

export async function registerVaultDevice(
	vaultId: string | null,
	deviceName: string,
	clientType: string,
	clientVersion?: string
): Promise<VaultDevice> {
	return vaultSyncClient.post<VaultDevice>('/devices/register', {
		vault_id: vaultId ?? null,
		device_name: deviceName,
		client_type: clientType,
		client_version: clientVersion
	});
}
