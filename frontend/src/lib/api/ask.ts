import { apiClient } from './client';

export type AskScope =
	| { type: 'workspace' }
	| { type: 'folder'; resourceRef: string }
	| { type: 'note'; resourceRef: string }
	| { type: 'chatChannel'; communityId: string; channelId: string };

export interface AskRequest {
	question: string;
	workspace_id: string;
	scope: AskScope;
	result_limit?: number;
}

export interface AskCitation {
	resource_ref: string;
	title: string;
	location?: string | null;
	provenance: Record<string, string | null | undefined>;
}

export interface AskResponse {
	answer: string;
	citations: AskCitation[];
	source_count: number;
	grounded: boolean;
	insufficient_evidence: boolean;
	run_id: string;
}

export interface OpenCitationResponse {
	resource_ref: string;
	display_name: string;
	media_type?: string | null;
	size?: number | null;
	updated_at?: string | null;
	available: boolean;
}

export function askHref(scope: Exclude<AskScope, { type: 'workspace' }>): string {
	const params = new URLSearchParams();
	if (scope.type === 'folder' || scope.type === 'note') {
		params.set('scope', scope.type);
		params.set('resourceRef', scope.resourceRef);
	} else {
		params.set('scope', 'chat');
		params.set('communityId', scope.communityId);
		params.set('channelId', scope.channelId);
	}
	return `/ask?${params.toString()}`;
}

export function askWorkspace(request: AskRequest): Promise<AskResponse> {
	return apiClient.post<AskResponse>('/memory/ask', request);
}

export function openAskCitation(resource_ref: string): Promise<OpenCitationResponse> {
	return apiClient.post<OpenCitationResponse>('/memory/citations/open', { resource_ref });
}
