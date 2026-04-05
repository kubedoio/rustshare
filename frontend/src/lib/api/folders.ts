import { apiClient } from './client';
import type { Folder, File } from './types';

export interface FolderContents {
  folders: Folder[];
  files: File[];
}

export interface FolderTreeNode {
  id: string;
  name: string;
  path: string;
  parent_folder_id: string | null;
  children: FolderTreeNode[];
}

export interface FolderTree {
  folder: {
    id: string;
    name: string;
    path: string;
    parent_folder_id: string | null;
    owner_id: string;
    created_at: string;
    updated_at: string;
    tenant_id: string;
    ancestor_ids: string[] | null;
    // Share info
    is_shared: boolean;
    share_count: number;
    share_expires_at: string | null;
  };
  subfolders: FolderTree[];
}

export async function createFolder(name: string, parentFolderId: string | null): Promise<Folder> {
  return apiClient.post<Folder>('/folders', {
    name,
    parent_folder_id: parentFolderId
  });
}

export async function getFolder(folderId: string): Promise<Folder> {
  return apiClient.get<Folder>(`/folders/${folderId}`);
}

export async function getFolderContents(folderId: string | null): Promise<FolderContents> {
  if (!folderId) {
    // Get root contents
    return apiClient.get<FolderContents>('/folders/root/contents');
  }
  return apiClient.get<FolderContents>(`/folders/${folderId}/contents`);
}

/**
 * Get contents of a shared folder (bypasses ownership check)
 * Use this when accessing folders shared with the current user
 */
export async function getSharedFolderContents(folderId: string): Promise<FolderContents> {
  return apiClient.get<FolderContents>(`/shares/folders/${folderId}/contents`);
}

export async function getFolderTree(): Promise<FolderTree> {
  return apiClient.get<FolderTree>('/folders/tree');
}

export async function renameFolder(folderId: string, newName: string): Promise<void> {
  return apiClient.post<void>(`/folders/${folderId}/rename`, { new_name: newName });
}

export async function moveFolder(folderId: string, targetFolderId: string | null): Promise<void> {
  return apiClient.post<void>(`/folders/${folderId}/move`, {
    target_parent_id: targetFolderId
  });
}

export async function deleteFolder(folderId: string): Promise<void> {
  return apiClient.delete<void>(`/folders/${folderId}`);
}

export async function permanentlyDeleteFolder(folderId: string): Promise<void> {
  return apiClient.delete<void>(`/folders/${folderId}/permanent`);
}

export async function restoreFolderFromTrash(folderId: string): Promise<void> {
  return apiClient.post<void>(`/folders/${folderId}/restore-from-trash`, null);
}

export async function setFolderStarred(folderId: string, starred: boolean): Promise<void> {
  return apiClient.patch<void>(`/folders/${folderId}/star`, { starred });
}
