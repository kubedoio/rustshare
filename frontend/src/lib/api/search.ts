import { apiClient } from './client';

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
