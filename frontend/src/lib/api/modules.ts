import { apiClient } from './client';
import type {
	ModuleConfig,
	CreateFromTemplateRequest,
	CreateFromTemplateResponse,
	ModuleSummary
} from './types';
import { normalizeModuleConfig } from '$lib/modules/workspaceSurface';

interface EnabledModulesResponse {
	modules: ModuleConfig[];
}

interface ModuleDetailResponse {
	module: ModuleConfig;
}

type LegacyModuleDetailResponse = ModuleConfig;

interface ModuleSummaryResponse {
	summary: ModuleSummary;
}

export async function listEnabledModules(): Promise<ModuleConfig[]> {
	const response = await apiClient.get<EnabledModulesResponse>('/modules');
	return response.modules.map(normalizeModuleConfig);
}

export async function getModule(key: string): Promise<ModuleConfig> {
	const response = await apiClient.get<ModuleDetailResponse | LegacyModuleDetailResponse>(
		`/modules/${key}`
	);
	return normalizeModuleConfig('module' in response ? response.module : response);
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
