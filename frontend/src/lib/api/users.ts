import { apiClient } from './client';
import type { Theme } from '$lib/stores/theme';

export interface UserProfile {
  id: string;
  username: string;
  display_name: string;
  email: string;
  is_admin: boolean;
  storage_quota: number;
  theme: Theme;
  created_at: string;
  updated_at: string;
}

export interface UpdateThemeResponse {
  theme: Theme;
}

/**
 * Get the current user's profile.
 */
export async function getUserProfile(): Promise<UserProfile> {
  const response = await apiClient.get<UserProfile>('/users/me');
  return response;
}

/**
 * Update the current user's theme preference.
 */
export async function updateUserTheme(theme: Theme): Promise<UpdateThemeResponse> {
  const response = await apiClient.patch<UpdateThemeResponse>('/users/me/theme', { theme });
  return response;
}
