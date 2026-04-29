import { apiClient } from './client';
import type { ModuleConfig, CreateFromTemplateRequest, CreateFromTemplateResponse } from './types';

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
