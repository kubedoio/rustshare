import { apiClient } from './client';
import type { ModuleConfig } from './types';

export interface CreateFromTemplateRequest {
	template_key: string;
	name: string;
	parent_folder_id?: string | null;
}

export interface CreateFromTemplateResponse {
	object_id: string;
	object_type: 'file' | 'folder';
	path: string;
}

export async function listEnabledModules(): Promise<ModuleConfig[]> {
	return apiClient.get<ModuleConfig[]>('/modules');
}

export async function getModule(key: string): Promise<ModuleConfig> {
	return apiClient.get<ModuleConfig>(`/modules/${key}`);
}

export async function createFromTemplate(
	request: CreateFromTemplateRequest
): Promise<CreateFromTemplateResponse> {
	return apiClient.post<CreateFromTemplateResponse>('/modules/from-template', request);
}
