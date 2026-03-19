import { apiClient } from './client';
import type { User } from './types';

interface LoginRequest {
  email: string;
  password: string;
}

interface LoginResponse {
  user: User;
  session_expires_at: string;
}

export async function login(email: string, password: string): Promise<LoginResponse> {
  return apiClient.post<LoginResponse>('/auth/login', { email, password });
}

export async function logout(): Promise<void> {
  await apiClient.post<null>('/auth/logout', null);
}
