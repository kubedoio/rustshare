import { apiClient } from './client';
import { getFile, getFileContent, editFile, renameFile } from './files';

export interface StandupMetadata {
	kind: string;
	title: string;
	date: string;
	created_at: string;
	updated_at: string;
}

export interface StandupRecord {
	id: string;
	name: string;
	path: string;
	content: string;
	metadata: StandupMetadata;
	owner_id: string;
	created_at: string;
	updated_at: string;
}

export interface StandupSummary {
	id: string;
	name: string;
	path: string;
	metadata: StandupMetadata;
	modified_at: string;
}

export const standupsApi = {
	list: async () => {
		return apiClient.get<StandupSummary[]>('/standups');
	},

	get: async (id: string) => {
		return apiClient.get<StandupRecord>(`/standups/${id}`);
	},

	create: async (req: { title: string; date: string; content: string }) => {
		return apiClient.post<StandupRecord>('/standups', req);
	},

	update: async (id: string, req: { title?: string; content?: string }) => {
		return apiClient.put<StandupRecord>(`/standups/${id}`, req);
	},

	// Fallback file-level ops for legacy single-file standups
	getLegacy: async (id: string) => {
		const [file, content] = await Promise.all([getFile(id), getFileContent(id)]);
		return {
			...file,
			content,
			metadata: { title: file.name }
		};
	},

	updateLegacy: async (id: string, req: { title?: string; content?: string }) => {
		if (req.content !== undefined) {
			await editFile(id, req.content, 'overwrite');
		}
		if (req.title !== undefined && req.title !== '') {
			await renameFile(id, req.title);
		}
		const [file, content] = await Promise.all([getFile(id), getFileContent(id)]);
		return {
			...file,
			content,
			metadata: { title: file.name }
		};
	}
};
