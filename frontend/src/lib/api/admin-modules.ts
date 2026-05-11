import { apiClient } from './client';
import type {
	ModuleConfig,
	TemplateConfig,
	TemplateDefaultFile,
	TemplateUiConfig,
	ModuleUiConfig
} from './types';
import { normalizeModuleConfig } from '$lib/modules/workspaceSurface';

interface AdminModulesResponse {
	modules: ModuleConfig[];
}

interface AdminTemplatesResponse {
	templates: TemplateConfig[];
}

export interface CreateTemplateRequest {
	template_key: string;
	name: string;
	module_key: string;
	description: string;
	ui_config?: TemplateUiConfig;
	folder_structure: string[];
	default_files: TemplateDefaultFile[];
	metadata_schema: Record<string, unknown>;
	renderer?: string | null;
	visibility_policy: string;
	module_config?: Record<string, unknown>;
}

export interface UpdateModuleRequest {
	display_name?: string;
	description?: string;
	icon?: string;
	root_path?: string;
	renderer?: string | null;
	default_template?: string | null;
	ui_config?: ModuleUiConfig;
}

export async function listAdminModules(): Promise<ModuleConfig[]> {
	const response = await apiClient.get<AdminModulesResponse>('/admin/modules');
	return response.modules.map(normalizeModuleConfig);
}

export async function getAdminModule(key: string): Promise<ModuleConfig> {
	return normalizeModuleConfig(await apiClient.get<ModuleConfig>(`/admin/modules/${key}`));
}

export async function enableModule(key: string): Promise<ModuleConfig> {
	return normalizeModuleConfig(
		await apiClient.post<ModuleConfig>(`/admin/modules/${key}/enable`, {})
	);
}

export async function disableModule(key: string): Promise<ModuleConfig> {
	return normalizeModuleConfig(
		await apiClient.post<ModuleConfig>(`/admin/modules/${key}/disable`, {})
	);
}

export async function updateModule(
	key: string,
	updates: UpdateModuleRequest
): Promise<ModuleConfig> {
	return normalizeModuleConfig(
		await apiClient.patch<ModuleConfig>(`/admin/modules/${key}`, updates)
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
	return apiClient.delete<void>(`/admin/templates/${key}`);
}

export async function duplicateTemplate(key: string): Promise<TemplateConfig> {
	return apiClient.post<TemplateConfig>(`/admin/templates/${key}/duplicate`, {});
}
