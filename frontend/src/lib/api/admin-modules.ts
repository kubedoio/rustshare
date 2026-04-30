import { apiClient } from './client';
import type {
	ModuleConfig,
	TemplateConfig,
	TemplateDefaultFile,
	TemplateUiConfig,
	ModuleUiConfig
} from './types';

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
	return response.modules;
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
	updates: UpdateModuleRequest
): Promise<ModuleConfig> {
	return apiClient.patch<ModuleConfig>(`/admin/modules/${key}`, updates);
}

export async function listAdminTemplates(): Promise<TemplateConfig[]> {
	const response = await apiClient.get<AdminTemplatesResponse>('/admin/templates');
	return response.templates;
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

export async function duplicateTemplate(key: string, newKey: string): Promise<TemplateConfig> {
	const original = await getAdminTemplate(key);
	return createTemplate({
		template_key: newKey,
		name: `${original.name} (Copy)`,
		module_key: original.module_key,
		description: original.description,
		ui_config: original.ui_config,
		folder_structure: original.folder_structure,
		default_files: original.default_files,
		metadata_schema: original.metadata_schema,
		renderer: original.renderer,
		visibility_policy: original.visibility_policy
	});
}
