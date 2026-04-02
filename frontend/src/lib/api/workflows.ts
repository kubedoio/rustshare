import { apiClient } from './client';

export interface Workflow {
	id: string;
	key: string;
	name: string;
	trigger_type: string;
	status: 'active' | 'draft';
	subject?: string;
	body?: string;
	terms_enabled: boolean;
	terms_text?: string;
	created_at: string;
	updated_at: string;
	updated_by?: string | null;
}

export interface UpdateWorkflowRequest {
	subject?: string;
	body?: string;
	terms_enabled?: boolean;
	terms_text?: string;
	status?: 'active' | 'draft';
}

export const listWorkflows = () => apiClient.get<Workflow[]>('/admin/workflows');
export const getWorkflow = (id: string) => apiClient.get<Workflow>(`/admin/workflows/${id}`);
export const updateWorkflow = (id: string, data: UpdateWorkflowRequest) =>
	apiClient.put<Workflow>(`/admin/workflows/${id}`, data);
export const enableWorkflow = (id: string) =>
	apiClient.post<Workflow>(`/admin/workflows/${id}/enable`);
export const disableWorkflow = (id: string) =>
	apiClient.post<Workflow>(`/admin/workflows/${id}/disable`);
