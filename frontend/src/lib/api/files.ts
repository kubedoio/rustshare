import { apiClient } from './client';
import type { File, FileVersion } from './types';

export async function listAllFiles(): Promise<File[]> {
  return apiClient.get<File[]>('/files');
}

export async function uploadFile(folderId: string | null, file: globalThis.File): Promise<File> {
  const formData = new FormData();
  formData.append('file', file);
  formData.append('name', file.name);

  // Only append parent_folder_id if it's not null
  if (folderId) {
    formData.append('parent_folder_id', folderId);
  }

  return apiClient.post<File>('/files/upload', formData);
}

export async function getFile(fileId: string): Promise<File> {
  return apiClient.get<File>(`/files/${fileId}`);
}

export async function downloadFile(fileId: string): Promise<{ url: string }> {
  return apiClient.get<{ url: string }>(`/files/${fileId}/download`);
}

export async function renameFile(fileId: string, newName: string): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/rename`, { new_name: newName });
}

export async function moveFile(fileId: string, targetFolderId: string | null): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/move`, { target_folder_id: targetFolderId });
}

export async function deleteFile(fileId: string): Promise<void> {
  return apiClient.delete<void>(`/files/${fileId}`);
}

export async function getFileVersions(fileId: string): Promise<FileVersion[]> {
  return apiClient.get<FileVersion[]>(`/files/${fileId}/versions`);
}

export async function restoreFileVersion(
  fileId: string,
  versionNumber: number
): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/restore`, { version_number: versionNumber });
}
