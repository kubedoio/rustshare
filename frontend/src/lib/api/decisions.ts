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

async function fetchAllDecisionPages(): Promise<DecisionSummary[]> {
	const PAGE_SIZE = 100;
	const decisions: DecisionSummary[] = [];
	let page = 1;

	while (true) {
		const batch = await apiClient.get<DecisionSummary[]>(
			`/decisions?page=${page}&per_page=${PAGE_SIZE}`
		);
		decisions.push(...batch);

		if (batch.length < PAGE_SIZE) {
			return decisions;
		}

		page += 1;
	}
}

export const decisionsApi = {
	list: async (limit?: number) => {
		// Preserve the original unbounded behaviour for callers that do not pass
		// a limit by walking all pages. Callers that pass a limit only want a
		// single slice.
		if (limit === undefined) {
			return fetchAllDecisionPages();
		}
		return apiClient.get<DecisionSummary[]>(`/decisions?per_page=${limit}`);
	},

	get: async (id: string) => {
		return apiClient.get<Decision>(`/decisions/${id}`);
	},

	create: async (req: { title: string; category: string; content: string }) => {
		return apiClient.post<Decision>('/decisions', req);
	},

	update: async (id: string, req: { title?: string; content?: string; status?: string }) => {
		return apiClient.put<Decision>(`/decisions/${id}`, req);
	},

	rename: async (id: string, req: { title: string }) => {
		return apiClient.post<Decision>(`/decisions/${id}/rename`, req);
	}
};
