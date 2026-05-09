import { apiClient } from './client';

export interface SearchResultItem {
	id: string;
	name: string;
	path: string;
	resource_type: string;
	owner_id: string;
	permission: string;
}

export interface SearchResponse {
	results: SearchResultItem[];
	count: number;
	query: string;
}

export async function searchResources(
	query: string,
	limit: number = 50,
	signal?: AbortSignal
): Promise<SearchResponse> {
	const params = new URLSearchParams();
	params.set('q', query);
	params.set('limit', String(limit));
	return apiClient.request<SearchResponse>(`/search?${params.toString()}`, {
		method: 'GET',
		signal
	});
}
