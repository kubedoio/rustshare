import { ApiError } from "./types";

const CSRF_HEADER_NAME = "X-Rustshare-Csrf";

export class ApiClient {
  constructor(private baseURL: string) {}

  async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
    const method = (options?.method || "GET").toUpperCase();
    const headers: Record<string, string> = {
      ...((options?.headers as Record<string, string>) || {}),
    };

    // Add Content-Type for JSON bodies (unless multipart form)
    if (options?.body && !(options.body instanceof FormData)) {
      headers["Content-Type"] = "application/json";
    }

    if (requiresCsrfHeader(method) && !headers[CSRF_HEADER_NAME]) {
      headers[CSRF_HEADER_NAME] = "1";
    }

    const response = await fetch(`${this.baseURL}${endpoint}`, {
      ...options,
      headers,
      credentials: "include",
    });

    // Handle 401 Unauthorized
    if (response.status === 401) {
      throw new ApiError(401, "Unauthorized");
    }

    // Handle other errors
    if (!response.ok) {
      let errorMessage = "Request failed";
      try {
        const errorData = await response.json();
        errorMessage = errorData.error || errorData.message || errorMessage;
      } catch {
        errorMessage = response.statusText || errorMessage;
      }
      throw new ApiError(response.status, errorMessage);
    }

    // Handle 204 No Content
    if (response.status === 204) {
      return null as T;
    }

    return response.json();
  }

  async get<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: "GET" });
  }

  async post<T>(
    endpoint: string,
    body?: Record<string, unknown> | FormData | null,
  ): Promise<T> {
    return this.request<T>(endpoint, {
      method: "POST",
      body: body instanceof FormData ? body : JSON.stringify(body),
    });
  }

  async put<T>(
    endpoint: string,
    body?: Record<string, unknown> | FormData | null,
  ): Promise<T> {
    return this.request<T>(endpoint, {
      method: "PUT",
      body: body instanceof FormData ? body : JSON.stringify(body),
    });
  }

  async delete<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: "DELETE" });
  }

  async patch<T>(
    endpoint: string,
    body?: Record<string, unknown> | FormData | null,
  ): Promise<T> {
    return this.request<T>(endpoint, {
      method: "PATCH",
      body: body instanceof FormData ? body : JSON.stringify(body),
    });
  }
}

function requiresCsrfHeader(method: string): boolean {
  return !["GET", "HEAD", "OPTIONS", "TRACE"].includes(method);
}

// Create singleton instance
const API_URL = import.meta.env.VITE_API_URL || "http://localhost:8080/api/v1";
export const apiClient = new ApiClient(API_URL);
