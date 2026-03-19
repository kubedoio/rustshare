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
  };
  subfolders: FolderTree[];
  files: any[];
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
