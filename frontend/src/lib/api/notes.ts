import { apiClient } from './client';
import type { Note, NoteMetadata, NoteSummary } from './types';

export interface CreateNoteRequest {
	title?: string;
	parent_folder_id?: string | null;
	content?: string;
}

export interface CreateNoteResponse {
	id: string;
	name: string;
	path: string;
	content: string;
	metadata: NoteMetadata;
	parent_folder_id: string | null;
	current_version: number;
	created_at: string;
	modified_at: string;
	public_url: string | null;
}

export interface SaveNoteRequest {
	content: string;
}

export interface SaveNoteResponse {
	id: string;
	current_version: number;
	modified_at: string;
	excerpt: string;
}

export interface RenameNoteRequest {
	title: string;
}

export interface MoveNoteRequest {
	target_folder_id: string | null;
}

export interface VisibilityResponse {
	id: string;
	visibility: 'private' | 'public';
	public_share_id: string | null;
	public_url: string | null;
}

export interface PublicNoteResponse {
	title: string;
	content: string;
	excerpt: string;
	created_at: string;
	updated_at: string;
}

export interface RecentNotesResponse {
	notes: NoteSummary[];
}

export async function createNote(request: CreateNoteRequest): Promise<CreateNoteResponse> {
	return apiClient.post<CreateNoteResponse>('/notes', request);
}

export async function getNote(noteId: string): Promise<Note> {
	return apiClient.get<Note>(`/notes/${noteId}`);
}

export async function saveNote(
	noteId: string,
	request: SaveNoteRequest
): Promise<SaveNoteResponse> {
	return apiClient.put<SaveNoteResponse>(`/notes/${noteId}`, request);
}

export async function renameNote(noteId: string, request: RenameNoteRequest): Promise<Note> {
	return apiClient.post<Note>(`/notes/${noteId}/rename`, request);
}

export async function moveNote(noteId: string, request: MoveNoteRequest): Promise<Note> {
	return apiClient.post<Note>(`/notes/${noteId}/move`, request);
}

export async function deleteNote(noteId: string): Promise<void> {
	return apiClient.delete<void>(`/notes/${noteId}`);
}

export async function listNotes(limit?: number): Promise<NoteSummary[]> {
	const query = limit !== undefined ? `?limit=${limit}` : '';
	return apiClient.get<NoteSummary[]>(`/notes${query}`);
}

export async function listRecentNotes(folderName?: string): Promise<RecentNotesResponse> {
	const query = folderName ? `?folder_name=${encodeURIComponent(folderName)}` : '';
	return apiClient.get<RecentNotesResponse>(`/notes/recent${query}`);
}

export async function toggleVisibility(noteId: string): Promise<VisibilityResponse> {
	return apiClient.post<VisibilityResponse>(`/notes/${noteId}/visibility`, {});
}

export async function getPublicNote(shareId: string): Promise<PublicNoteResponse> {
	const response = await fetch(`/api/v1/public/notes/${shareId}`, {
		credentials: 'include'
	});
	if (!response.ok) {
		throw new Error('Failed to load public note');
	}
	return response.json();
}
