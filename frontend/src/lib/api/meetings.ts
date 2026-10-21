import { apiClient } from './client';

export interface MeetingMetadata {
	kind: string;
	title: string;
	date: string;
	team: string;
	attendees: string[];
	calendar_event_id?: string | null;
	copied_from_meeting_id?: string | null;
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

export interface MeetingResourceLink {
	id: string;
	available: boolean;
	created_at: string;
	resource_ref?: string;
	display_name?: string;
	media_type?: string;
	resource_kind?: 'file' | 'note';
	open_url?: string;
}

export interface MeetingResourceLinkPage {
	items: MeetingResourceLink[];
	total: number;
	page: number;
	per_page: number;
}

export interface MeetingResourceCandidate {
	resource_ref: string;
	title: string;
	location?: string;
	source_application: string;
	source_type: string;
}

export interface MeetingDecisionLink {
	id: string;
	available: boolean;
	created_at: string;
	decision?: {
		id: string;
		name: string;
		title: string;
		status: string;
		decision_date?: string | null;
		owner_id: string;
		owner_display_name?: string | null;
		open_url: string;
	};
}

export interface MeetingDecisionPage {
	items: MeetingDecisionLink[];
	total: number;
	page: number;
	per_page: number;
}

export interface CreateMeetingDecision {
	request_id: string;
	title: string;
	category: string;
	content: string;
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

	listDecisions: async (id: string, page = 1, perPage = 50) => {
		return apiClient.get<MeetingDecisionPage>(
			`/meetings/${id}/decisions?page=${page}&per_page=${perPage}`
		);
	},

	createDecision: async (id: string, req: CreateMeetingDecision) => {
		return apiClient.post<MeetingDecisionLink>(`/meetings/${id}/decisions`, req);
	},

	linkDecision: async (id: string, decisionId: string) => {
		return apiClient.post<MeetingDecisionLink>(`/meetings/${id}/decisions/links`, {
			decision_id: decisionId
		});
	},

	unlinkDecision: async (id: string, linkId: string) => {
		return apiClient.delete(`/meetings/${id}/decisions/${linkId}`);
	},

	listResources: async (id: string, page = 1, perPage = 50) => {
		return apiClient.get<MeetingResourceLinkPage>(
			`/meetings/${id}/resources?page=${page}&per_page=${perPage}`
		);
	},

	linkResource: async (id: string, resourceRef: string) => {
		return apiClient.post<MeetingResourceLink>(`/meetings/${id}/resources`, {
			resource_ref: resourceRef
		});
	},

	unlinkResource: async (id: string, linkId: string) => {
		return apiClient.delete(`/meetings/${id}/resources/${linkId}`);
	},

	searchResources: async (query: string) => {
		const response = await apiClient.post<{
			results: MeetingResourceCandidate[];
			total: number;
		}>('/search', { query, limit: 30, sources: ['files'] });
		return response.results;
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
