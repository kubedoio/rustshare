import { apiClient } from './client';

export interface MeetingMetadata {
	kind: string;
	title: string;
	date: string;
	team: string;
	attendees: string[];
	created_at: string;
	updated_at: string;
}

export interface MeetingNote {
	id: string;
	name: string;
	path: string;
	content: string;
	metadata: MeetingMetadata;
	owner_id: string;
	created_at: string;
	updated_at: string;
}

export interface MeetingSummary {
	id: string;
	name: string;
	path: string;
	parent_folder_id: string | null;
	metadata: MeetingMetadata;
	modified_at: string;
}

async function fetchAllMeetingPages(): Promise<MeetingSummary[]> {
	const PAGE_SIZE = 100;
	const meetings: MeetingSummary[] = [];
	let page = 1;

	while (true) {
		const batch = await apiClient.get<MeetingSummary[]>(
			`/meetings?page=${page}&per_page=${PAGE_SIZE}`
		);
		meetings.push(...batch);

		if (batch.length < PAGE_SIZE) {
			return meetings;
		}

		page += 1;
	}
}

export const meetingsApi = {
	list: async (limit?: number) => {
		// Preserve the original unbounded behaviour for callers that do not pass
		// a limit by walking all pages. Callers that pass a limit only want a
		// single slice.
		if (limit === undefined) {
			return fetchAllMeetingPages();
		}
		return apiClient.get<MeetingSummary[]>(`/meetings?per_page=${limit}`);
	},

	get: async (id: string) => {
		return apiClient.get<MeetingNote>(`/meetings/${id}`);
	},

	create: async (req: { title: string; team: string; date: string; content: string }) => {
		return apiClient.post<MeetingNote>('/meetings', req);
	},

	update: async (id: string, req: { title?: string; content?: string; attendees?: string[] }) => {
		return apiClient.put<MeetingNote>(`/meetings/${id}`, req);
	},

	rename: async (id: string, req: { title: string }) => {
		return apiClient.put<MeetingNote>(`/meetings/${id}`, { title: req.title });
	},

	delete: async (id: string) => {
		return apiClient.delete(`/meetings/${id}`);
	},

	move: async (id: string, req: { target_folder_id: string | null }) => {
		return apiClient.post<MeetingNote>(`/meetings/${id}/move`, req);
	},

	duplicate: async (id: string) => {
		return apiClient.post<MeetingNote>(`/meetings/${id}/duplicate`);
	}
};
