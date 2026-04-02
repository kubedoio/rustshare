import { apiClient } from './client';

export interface FeaturesResponse {
	invite_enabled: boolean;
}

export const getFeatures = () => apiClient.get<FeaturesResponse>('/features');
