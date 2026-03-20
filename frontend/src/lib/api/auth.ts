import { apiClient } from "./client";
import type { User } from "./types";

interface LoginRequest {
  email: string;
  password: string;
}

interface LoginResponse {
  token?: string;
  user: User;
}

export async function login(
  email: string,
  password: string,
): Promise<LoginResponse> {
  return apiClient.post<LoginResponse>("/auth/login", { email, password });
}

export async function logout(): Promise<void> {
  try {
    await apiClient.post<void>("/auth/logout", null);
  } catch (error) {
    console.error("Failed to logout cleanly:", error);
  }

  if (typeof window !== "undefined") {
    window.location.href = "/login";
  }
}
