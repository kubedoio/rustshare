import { apiClient } from './client';
import type { ModuleConfig, TemplateConfig, TemplateDefaultFile } from './types';

export interface CreateTemplateRequest {
	template_key: string;
	name: string;
	module_key: string;
	description: string;
	folder_structure: string[];
	default_files: TemplateDefaultFile[];
	metadata_schema: Record<string, unknown>;
	renderer?: string | null;
	visibility_policy: string;
}

export async function listAdminModules(): Promise<ModuleConfig[]> {
	return apiClient.get<ModuleConfig[]>('/admin/modules');
}

export async function getAdminModule(key: string): Promise<ModuleConfig> {
	return apiClient.get<ModuleConfig>(`/admin/modules/${key}`);
}

export async function enableModule(key: string): Promise<ModuleConfig> {
	return apiClient.post<ModuleConfig>(`/admin/modules/${key}/enable`, {});
}

export async function disableModule(key: string): Promise<ModuleConfig> {
	return apiClient.post<ModuleConfig>(`/admin/modules/${key}/disable`, {});
}

export async function updateModule(
	key: string,
	updates: Partial<ModuleConfig>
): Promise<ModuleConfig> {
	return apiClient.patch<ModuleConfig>(`/admin/modules/${key}`, updates);
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
	return apiClient.patch<TemplateConfig>(`/admin/templates/${key}`, updates);
}

export async function deleteTemplate(key: string): Promise<void> {
	return apiClient.delete<void>(`/admin/templates/${key}`);
}
