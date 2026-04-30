import { apiClient } from './client';
import type { WorkspaceSurfaceDefinition } from './types';

interface WorkspaceSurfaceResponse {
	surface: WorkspaceSurfaceDefinition;
}

export async function getWorkspaceSurface(): Promise<WorkspaceSurfaceDefinition> {
	const response = await apiClient.get<WorkspaceSurfaceResponse>('/workspace-surface');
	return response.surface;
}
