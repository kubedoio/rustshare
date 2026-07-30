import { ApiError } from './types';

const CSRF_HEADER_NAME = 'X-Rustshare-Csrf';
const CSRF_COOKIE_NAME = 'rustshare_csrf_token';

/**
 * Read the double-submit CSRF token from the cookie set by the server during login/OIDC.
 * Returns undefined when running outside a browser or if the cookie is absent.
 */
export function getCsrfToken(): string | undefined {
	if (typeof document === 'undefined') {
		return undefined;
	}
	const match = document.cookie.match(
		new RegExp('(?:^|; )' + encodeURIComponent(CSRF_COOKIE_NAME) + '=([^;]*)')
	);
	return match ? decodeURIComponent(match[1]) : undefined;
}

export class ApiClient {
	constructor(private baseURL: string) {}

	getBaseURL(): string {
		return this.baseURL;
	}

	private buildURL(endpoint: string): string {
		if (/^https?:\/\//i.test(endpoint)) {
			return endpoint;
		}

		const normalizedBase = this.baseURL.replace(/\/$/, '');
		const normalizedEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`;

		if (
			normalizedEndpoint === normalizedBase ||
			normalizedEndpoint.startsWith(`${normalizedBase}/`)
		) {
			return normalizedEndpoint;
		}

		try {
			const base = new URL(normalizedBase);
			if (
				base.pathname !== '/' &&
				(normalizedEndpoint === base.pathname ||
					normalizedEndpoint.startsWith(`${base.pathname.replace(/\/$/, '')}/`))
			) {
				return `${base.origin}${normalizedEndpoint}`;
			}
		} catch {
			// Relative base URLs are handled by string concatenation below.
		}

		return `${normalizedBase}${normalizedEndpoint}`;
	}

	private async executeFetch(endpoint: string, options?: RequestInit): Promise<Response> {
		const method = (options?.method || 'GET').toUpperCase();
		const headers: Record<string, string> = {
			...((options?.headers as Record<string, string>) || {})
		};

		// Add Content-Type for JSON bodies (unless multipart form or Blob)
		if (options?.body && !(options.body instanceof FormData) && !(options.body instanceof Blob)) {
			if (!headers['Content-Type']) {
				headers['Content-Type'] = 'application/json';
			}
		}

		// Add CSRF header if needed
		if (requiresCsrfHeader(method) && !headers[CSRF_HEADER_NAME]) {
			const csrfToken = getCsrfToken();
			if (csrfToken) {
				headers[CSRF_HEADER_NAME] = csrfToken;
			}
		}

		// Add Authorization header if token exists in sessionStorage
		if (typeof window !== 'undefined') {
			const token = window.sessionStorage.getItem('rustshare.websocket_token');
			if (token && !headers['Authorization']) {
				headers['Authorization'] = `Bearer ${token}`;
			}
		}

		const response = await fetch(this.buildURL(endpoint), {
			...options,
			headers,
			credentials: 'include'
		});

		// Handle 401 Unauthorized
		if (response.status === 401) {
			let errorMessage = 'Unauthorized';
			let details: Record<string, unknown> | undefined;
			try {
				const errorData = await response.json();
				if (errorData && typeof errorData === 'object') {
					details = errorData as Record<string, unknown>;
					if (typeof details.error === 'string') errorMessage = details.error;
					else if (typeof details.message === 'string') errorMessage = details.message;
				}
			} catch {
				// keep default message if body isn't JSON
			}
			throw new ApiError(401, errorMessage, details);
		}

		// Handle other errors
		if (!response.ok) {
			let errorMessage = 'Request failed';
			let details: Record<string, unknown> | undefined;
			try {
				const errorData = await response.json();
				if (errorData && typeof errorData === 'object') {
					details = errorData as Record<string, unknown>;
					if (typeof details.error === 'string') errorMessage = details.error;
					else if (typeof details.message === 'string') errorMessage = details.message;
				}
			} catch {
				errorMessage = response.statusText || errorMessage;
			}
			throw new ApiError(response.status, errorMessage, details);
		}

		return response;
	}

	async request<T>(endpoint: string, options?: RequestInit): Promise<T> {
		const response = await this.executeFetch(endpoint, options);
		return response.json();
	}

	async requestVoid(endpoint: string, options?: RequestInit): Promise<void> {
		await this.executeFetch(endpoint, options);
	}

	async requestText(endpoint: string, options?: RequestInit): Promise<string> {
		const response = await this.executeFetch(endpoint, options);
		return response.text();
	}

	async requestTextWithHeaders(
		endpoint: string,
		options?: RequestInit
	): Promise<{ text: string; headers: Headers }> {
		const response = await this.executeFetch(endpoint, options);
		return { text: await response.text(), headers: response.headers };
	}

	async get<T>(endpoint: string): Promise<T> {
		return this.request<T>(endpoint, { method: 'GET' });
	}

	async post<T>(endpoint: string, body?: object | FormData | null): Promise<T> {
		return this.request<T>(endpoint, {
			method: 'POST',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}

	async postVoid(endpoint: string, body?: object | FormData | null): Promise<void> {
		return this.requestVoid(endpoint, {
			method: 'POST',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}

	async put<T>(endpoint: string, body?: object | FormData | null): Promise<T> {
		return this.request<T>(endpoint, {
			method: 'PUT',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}

	async delete(endpoint: string, body?: object | FormData | null): Promise<void> {
		return this.requestVoid(endpoint, {
			method: 'DELETE',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}

	async patch<T>(endpoint: string, body?: object | FormData | null): Promise<T> {
		return this.request<T>(endpoint, {
			method: 'PATCH',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}

	async patchVoid(endpoint: string, body?: object | FormData | null): Promise<void> {
		return this.requestVoid(endpoint, {
			method: 'PATCH',
			body: body instanceof FormData ? body : JSON.stringify(body)
		});
	}
}

function requiresCsrfHeader(method: string): boolean {
	return !['GET', 'HEAD', 'OPTIONS', 'TRACE'].includes(method);
}

// Create singleton instance
const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080/api/v1';
export const apiClient = new ApiClient(API_URL);
