import { apiClient } from './client';
import type {
	ApplicationShellEntry,
	CreateFromTemplateRequest,
	CreateFromTemplateResponse,
	ApplicationSummary
} from './types';

interface EnabledApplicationsResponse {
	applications: ApplicationShellEntry[];
}

interface ApplicationDetailResponse {
	application: ApplicationShellEntry;
}

interface ApplicationSummaryResponse {
	summary: ApplicationSummary;
}

export async function listEnabledApplications(): Promise<ApplicationShellEntry[]> {
	const response = await apiClient.get<EnabledApplicationsResponse>('/applications');
	return response.applications;
}

export async function getApplication(key: string): Promise<ApplicationShellEntry> {
	const response = await apiClient.get<ApplicationDetailResponse>(`/applications/${key}`);
	return response.application;
}

export async function createFromTemplate(
	request: CreateFromTemplateRequest
): Promise<CreateFromTemplateResponse> {
	return apiClient.post<CreateFromTemplateResponse>('/applications/from-template', request);
}

export async function getApplicationSummary(applicationId: string): Promise<ApplicationSummary> {
	const response = await apiClient.get<ApplicationSummaryResponse>(
		`/applications/${applicationId}/summary`
	);
	return response.summary;
}
