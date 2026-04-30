import { apiClient } from './client';
import type {
	ModuleConfig,
	CreateFromTemplateRequest,
	CreateFromTemplateResponse,
	ModuleSummary
} from './types';

interface EnabledModulesResponse {
	modules: ModuleConfig[];
}

interface ModuleDetailResponse {
	module: ModuleConfig;
}

interface ModuleSummaryResponse {
	summary: ModuleSummary;
}

export async function listEnabledModules(): Promise<ModuleConfig[]> {
	const response = await apiClient.get<EnabledModulesResponse>('/modules');
	return response.modules;
}

export async function getModule(key: string): Promise<ModuleConfig> {
	const response = await apiClient.get<ModuleDetailResponse>(`/modules/${key}`);
	return response.module;
}

export async function createFromTemplate(
	request: CreateFromTemplateRequest
): Promise<CreateFromTemplateResponse> {
	return apiClient.post<CreateFromTemplateResponse>('/modules/from-template', request);
}

export async function getModuleSummary(moduleKey: string): Promise<ModuleSummary> {
	const response = await apiClient.get<ModuleSummaryResponse>(`/modules/${moduleKey}/summary`);
	return response.summary;
}
