import { apiClient } from './client';

export interface ActivityItem {
	id: string;
	action: string;
	resource_type: string;
	resource_id: string;
	resource_name: string | null;
	actor_id: string;
	timestamp: string;
}

export interface ActivityCursor {
	before_timestamp: string;
	before_id: string;
}

export interface ListActivityResponse {
	items: ActivityItem[];
	next_cursor: ActivityCursor | null;
}

export interface ListActivityParams {
	limit?: number;
	before_timestamp?: string;
	before_id?: string;
}

export async function listActivity(params: ListActivityParams = {}): Promise<ListActivityResponse> {
	const searchParams = new URLSearchParams();
	if (params.limit !== undefined) {
		searchParams.set('limit', String(params.limit));
	}
	if (params.before_timestamp !== undefined) {
		searchParams.set('before_timestamp', params.before_timestamp);
	}
	if (params.before_id !== undefined) {
		searchParams.set('before_id', params.before_id);
	}
	const query = searchParams.toString();
	return apiClient.get<ListActivityResponse>(query ? `/activity?${query}` : '/activity');
}
