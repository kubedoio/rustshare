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
	metadata: MeetingMetadata;
	modified_at: string;
}

export const meetingsApi = {
	list: async () => {
		return apiClient.get<MeetingSummary[]>('/meetings');
	},

	get: async (id: string) => {
		return apiClient.get<MeetingNote>(`/meetings/${id}`);
	},

	create: async (req: { title: string; team: string; date: string; content: string }) => {
		return apiClient.post<MeetingNote>('/meetings', req);
	},

	update: async (id: string, req: { title?: string; content?: string; attendees?: string[] }) => {
		return apiClient.put<MeetingNote>(`/meetings/${id}`, req);
	}
};
