import { apiClient } from './client';
import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';

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

export interface UnifiedSearchResult {
	source_application: string;
	source_type: string;
	resource_ref: string;
	title: string;
	snippet?: string;
	location?: string;
	provenance: {
		file_id?: string;
		note_id?: string;
		meeting_id?: string;
		calendar_event_id?: string;
	};
}

export interface UnifiedSearchResponse {
	results: UnifiedSearchResult[];
	total: number;
}

export async function searchResources(
	query: string,
	limit: number = 50,
	signal?: AbortSignal
): Promise<SearchResponse> {
	const params = new URLSearchParams();
	params.set('q', query);
	params.set('limit', String(limit));
	const response = await apiClient.request<SearchResponse>(`/search?${params.toString()}`, {
		method: 'GET',
		signal
	});
	return {
		...response,
		results: filterUserVisibleEntries(response.results)
	};
}

export function unifiedSearch(
	query: string,
	limit: number = 20,
	signal?: AbortSignal
): Promise<UnifiedSearchResponse> {
	return apiClient.request<UnifiedSearchResponse>('/api/v1/search', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ query, limit, sources: ['files'] }),
		signal
	});
}
