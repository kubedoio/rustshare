import { apiClient } from './client';
import type {
	ApplicationConfig,
	TemplateConfig,
	TemplateDefaultFile,
	TemplateUiConfig,
	ApplicationUiConfig
} from './types';
import { normalizeApplicationConfig } from '$lib/applications/workspaceSurface';

interface AdminApplicationsResponse {
	applications: ApplicationConfig[];
}

interface AdminTemplatesResponse {
	templates: TemplateConfig[];
}

export interface CreateTemplateRequest {
	template_key: string;
	name: string;
	application_id: string;
	description: string;
	ui_config?: TemplateUiConfig;
	folder_structure: string[];
	default_files: TemplateDefaultFile[];
	metadata_schema: Record<string, unknown>;
	renderer?: string | null;
	visibility_policy: string;
	application_config?: Record<string, unknown>;
}

export interface UpdateApplicationRequest {
	display_name?: string;
	description?: string;
	icon?: string;
	root_path?: string;
	renderer?: string | null;
	default_template?: string | null;
	ui_config?: ApplicationUiConfig;
}

export async function listAdminApplications(): Promise<ApplicationConfig[]> {
	const response = await apiClient.get<AdminApplicationsResponse>('/admin/applications');
	return response.applications.map(normalizeApplicationConfig);
}

export async function getAdminApplication(key: string): Promise<ApplicationConfig> {
	return normalizeApplicationConfig(
		await apiClient.get<ApplicationConfig>(`/admin/applications/${key}`)
	);
}

export async function enableApplication(key: string): Promise<ApplicationConfig> {
	return normalizeApplicationConfig(
		await apiClient.post<ApplicationConfig>(`/admin/applications/${key}/enable`, {})
	);
}

export async function disableApplication(key: string): Promise<ApplicationConfig> {
	return normalizeApplicationConfig(
		await apiClient.post<ApplicationConfig>(`/admin/applications/${key}/disable`, {})
	);
}

export async function updateApplication(
	key: string,
	updates: UpdateApplicationRequest
): Promise<ApplicationConfig> {
	return normalizeApplicationConfig(
		await apiClient.patch<ApplicationConfig>(`/admin/applications/${key}`, updates)
	);
}

export async function listAdminTemplates(): Promise<TemplateConfig[]> {
	return apiClient.get<TemplateConfig[]>('/admin/templates');
}

export async function getAdminTemplate(key: string): Promise<TemplateConfig> {
	return apiClient.get<TemplateConfig>(`/admin/templates/${key}`);
}

export async function createTemplate(request: CreateTemplateRequest): Promise<TemplateConfig> {
	return apiClient.post<TemplateConfig>('/admin/templates', request);
}

export async function updateTemplate(
	key: string,
	updates: Partial<TemplateConfig>
): Promise<TemplateConfig> {
	return apiClient.put<TemplateConfig>(`/admin/templates/${key}`, updates);
}

export async function deleteTemplate(key: string): Promise<void> {
	return apiClient.delete(`/admin/templates/${key}`);
}

export async function duplicateTemplate(key: string): Promise<TemplateConfig> {
	return apiClient.post<TemplateConfig>(`/admin/templates/${key}/duplicate`, {});
}
