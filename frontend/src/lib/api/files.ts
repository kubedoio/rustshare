import { apiClient } from "./client";
import type { File, FileVersion } from "./types";

export async function listAllFiles(): Promise<File[]> {
  return apiClient.get<File[]>("/files");
}

export async function uploadFile(
  folderId: string | null,
  file: globalThis.File,
): Promise<File> {
  const formData = new FormData();
  formData.append("file", file);
  formData.append("name", file.name);

  // Only append parent_folder_id if it's not null
  if (folderId) {
    formData.append("parent_folder_id", folderId);
  }

  return apiClient.post<File>("/files/upload", formData);
}

export async function getFile(fileId: string): Promise<File> {
  return apiClient.get<File>(`/files/${fileId}`);
}

export async function downloadFile(fileId: string): Promise<{ url: string }> {
  // Use the new /content endpoint which returns the file with proper Content-Disposition header
  // This ensures downloaded files have their original filename instead of the storage ID
  return { url: `/api/v1/files/${fileId}/content` };
}

export async function renameFile(
  fileId: string,
  newName: string,
): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/rename`, { new_name: newName });
}

export async function moveFile(
  fileId: string,
  targetFolderId: string | null,
): Promise<void> {
  return apiClient.post<void>(`/files/${fileId}/move`, {
    target_folder_id: targetFolderId,
  });
}

export async function deleteFile(fileId: string): Promise<void> {
  return apiClient.delete<void>(`/files/${fileId}`);
}

export async function updateFile(
  fileId: string,
  file: globalThis.File,
  currentVersion: number,
): Promise<File> {
  const formData = new FormData();
  formData.append("file", file);
  const headers: Record<string, string> = {
    "If-Match": currentVersion.toString(),
    "X-Rustshare-Csrf": "1",
  };

  const response = await fetch(
    `${import.meta.env.VITE_API_URL || "http://localhost:8080/api/v1"}/files/${fileId}`,
    {
      method: "PUT",
      headers,
      body: formData,
      credentials: "include",
    },
  );

  if (!response.ok) {
    throw new Error(`Failed to update file: ${response.statusText}`);
  }

  return response.json();
}

export async function getFileVersions(fileId: string): Promise<FileVersion[]> {
  return apiClient.get<FileVersion[]>(`/files/${fileId}/versions`);
}

export async function restoreFileVersion(
  fileId: string,
  versionNumber: number,
  currentVersion: number,
): Promise<File> {
  const headers: Record<string, string> = {
    "If-Match": currentVersion.toString(),
    "Content-Type": "application/json",
  };

  const response = await apiClient.request<File>(`/files/${fileId}/restore`, {
    method: "POST",
    headers,
    body: JSON.stringify({ version: versionNumber }),
  });

  return response;
}
