import { apiClient } from './client';

export interface DecisionMetadata {
	kind: string;
	title: string;
	status: string;
	category: string;
	decision_date?: string;
	created_at: string;
	updated_at: string;
}

export interface Decision {
	id: string;
	name: string;
	path: string;
	content: string;
	metadata: DecisionMetadata;
	owner_id: string;
	created_at: string;
	updated_at: string;
}

export interface DecisionSummary {
	id: string;
	name: string;
	path: string;
	metadata: DecisionMetadata;
	modified_at: string;
}

export const decisionsApi = {
	list: async () => {
		return apiClient.get<DecisionSummary[]>('/decisions');
	},

	get: async (id: string) => {
		return apiClient.get<Decision>(`/decisions/${id}`);
	},

	create: async (req: { title: string; category: string; content: string }) => {
		return apiClient.post<Decision>('/decisions', req);
	},

	update: async (id: string, req: { title?: string; content?: string; status?: string }) => {
		return apiClient.put<Decision>(`/decisions/${id}`, req);
	}
};
