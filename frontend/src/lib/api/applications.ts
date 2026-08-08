import { apiClient } from './client';
import type {
	ApplicationConfig,
	CreateFromTemplateRequest,
	CreateFromTemplateResponse,
	ApplicationSummary
} from './types';
import { normalizeApplicationConfig } from '$lib/applications/workspaceSurface';

interface EnabledApplicationsResponse {
	applications: ApplicationConfig[];
}

interface ApplicationDetailResponse {
	application: ApplicationConfig;
}

interface ApplicationSummaryResponse {
	summary: ApplicationSummary;
}

export async function listEnabledApplications(): Promise<ApplicationConfig[]> {
	const response = await apiClient.get<EnabledApplicationsResponse>('/applications');
	return response.applications.map(normalizeApplicationConfig);
}

export async function getApplication(key: string): Promise<ApplicationConfig> {
	const response = await apiClient.get<ApplicationDetailResponse>(`/applications/${key}`);
	return normalizeApplicationConfig(response.application);
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
