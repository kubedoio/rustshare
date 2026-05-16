import { apiClient } from '../client';

export interface TemplateDefinition {
	id: string;
	template_key: string;
	name: string;
	module_key: string;
	version: string;
	description: string;
	ui_config: unknown;
	folder_structure: string[];
	default_files: unknown[];
	metadata_schema: unknown;
	renderer?: string;
	visibility_policy: string;
	ai_indexing_policy: unknown;
	audit_logging_policy: unknown;
	enabled: boolean;
	system_template: boolean;
	created_at: string;
	updated_at: string;
}

export async function listTemplates(): Promise<TemplateDefinition[]> {
	return apiClient.get<TemplateDefinition[]>('/admin/templates');
}

export async function getTemplate(key: string): Promise<TemplateDefinition> {
	return apiClient.get<TemplateDefinition>(`/admin/templates/${key}`);
}

export async function createTemplate(
	template: Partial<TemplateDefinition>
): Promise<TemplateDefinition> {
	return apiClient.post<TemplateDefinition>('/admin/templates', template);
}

export async function updateTemplate(
	key: string,
	template: Partial<TemplateDefinition>
): Promise<TemplateDefinition> {
	return apiClient.put<TemplateDefinition>(`/admin/templates/${key}`, template);
}

export async function deleteTemplate(key: string): Promise<void> {
	return apiClient.delete(`/admin/templates/${key}`);
}

export async function duplicateTemplate(key: string): Promise<TemplateDefinition> {
	return apiClient.post<TemplateDefinition>(`/admin/templates/${key}/duplicate`);
}
