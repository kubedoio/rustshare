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

async function fetchAllStandupPages(): Promise<StandupSummary[]> {
	const PAGE_SIZE = 100;
	const standups: StandupSummary[] = [];
	let page = 1;

	while (true) {
		const batch = await apiClient.get<StandupSummary[]>(
			`/standups?page=${page}&per_page=${PAGE_SIZE}`
		);
		standups.push(...batch);

		if (batch.length < PAGE_SIZE) {
			return standups;
		}

		page += 1;
	}
}

export const standupsApi = {
	list: async (limit?: number) => {
		// Preserve the original unbounded behaviour for callers that do not pass
		// a limit by walking all pages. Callers that pass a limit only want a
		// single slice.
		if (limit === undefined) {
			return fetchAllStandupPages();
		}
		return apiClient.get<StandupSummary[]>(`/standups?per_page=${limit}`);
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

	rename: async (id: string, req: { title: string }) => {
		return apiClient.put<StandupRecord>(`/standups/${id}`, { title: req.title });
	},

	delete: async (id: string) => {
		return apiClient.delete(`/standups/${id}`);
	},

	move: async (id: string, req: { target_folder_id: string | null }) => {
		return apiClient.post<StandupRecord>(`/standups/${id}/move`, req);
	},

	duplicate: async (id: string) => {
		return apiClient.post<StandupRecord>(`/standups/${id}/duplicate`);
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
