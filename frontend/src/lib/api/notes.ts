import { apiClient } from './client';
import type { Note, NoteMetadata, NoteSummary, NoteAttachment } from './types';

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
	color?: string | null;
	attachments?: NoteAttachment[];
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
	return apiClient.delete(`/notes/${noteId}`);
}

export async function duplicateNote(noteId: string): Promise<CreateNoteResponse> {
	return apiClient.post<CreateNoteResponse>(`/notes/${noteId}/duplicate`);
}

export async function listNotes(limit?: number): Promise<NoteSummary[]> {
	// When no limit is provided, preserve the original unbounded behaviour by
	// walking all pages. Callers that pass a limit only want a single slice.
	if (limit === undefined) {
		return fetchAllNotePages();
	}
	return apiClient.get<NoteSummary[]>(`/notes?per_page=${limit}`);
}

async function fetchAllNotePages(): Promise<NoteSummary[]> {
	const PAGE_SIZE = 100;
	const notes: NoteSummary[] = [];
	let page = 1;

	while (true) {
		const batch = await apiClient.get<NoteSummary[]>(
			`/notes?page=${page}&per_page=${PAGE_SIZE}`
		);
		notes.push(...batch);

		if (batch.length < PAGE_SIZE) {
			return notes;
		}

		page += 1;
	}
}

export async function listRecentNotes(folderName?: string): Promise<RecentNotesResponse> {
	const query = folderName ? `?folder_name=${encodeURIComponent(folderName)}` : '';
	return apiClient.get<RecentNotesResponse>(`/notes/recent${query}`);
}

export async function toggleVisibility(noteId: string): Promise<VisibilityResponse> {
	return apiClient.post<VisibilityResponse>(`/notes/${noteId}/visibility`, {});
}

export async function getPublicNote(shareId: string): Promise<PublicNoteResponse> {
	return apiClient.get<PublicNoteResponse>(`/public/notes/${shareId}`);
}
export const notesApi = {
	get: getNote,
	list: listNotes,
	create: createNote,
	update: async (
		id: string,
		req: { content: string; color?: string | null; attachments?: NoteAttachment[] }
	) => saveNote(id, req),
	delete: deleteNote,
	toggleVisibility
};
