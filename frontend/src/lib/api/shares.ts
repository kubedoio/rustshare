import { apiClient } from "./client";
import type { Share } from "./types";

// Request/Response Types

export interface CreateShareRequest {
  permissions: "View" | "Edit" | "Admin";
  password?: string;
  expires_at?: string; // ISO 8601
}

export interface CreateShareResponse {
  id: string;
  share_token: string;
  permissions: "View" | "Edit" | "Admin";
  password_protected: boolean;
  expires_at: string | null;
  share_url: string;
}

export interface ShareInfo {
  file_id: string;
  file_name: string;
  file_size: number;
  mime_type: string;
  password_protected: boolean;
  expires_at: string | null;
}

export interface ShareSessionRequest {
  password?: string;
}

export interface ShareSessionResponse {
  session_token: string;
  expires_at: string;
}

// Authenticated Share Operations

/**
 * Create a public share link for a file
 * POST /api/files/{file_id}/shares
 */
export async function createShare(
  fileId: string,
  request: CreateShareRequest,
): Promise<CreateShareResponse> {
  const response = await apiClient.post<Omit<CreateShareResponse, "share_url">>(
    `/files/${fileId}/shares`,
    request,
  );

  // Generate the full share URL on the client side
  const baseUrl = window.location.origin;
  const share_url = `${baseUrl}/share/${response.share_token}`;

  return {
    ...response,
    share_url,
  };
}

/**
 * List all share links for a file
 * GET /api/files/{file_id}/shares
 */
export async function listFileShares(fileId: string): Promise<Share[]> {
  return apiClient.get<Share[]>(`/files/${fileId}/shares`);
}

/**
 * List all shares owned by the current user
 * This is a workaround since the backend doesn't have a dedicated endpoint
 * We'll need to aggregate file shares from all files
 */
export async function listAllUserShares(): Promise<Share[]> {
  return apiClient.get<Share[]>("/shares");
}

/**
 * Revoke a share link
 * DELETE /api/shares/{id}
 * Note: This endpoint may not be implemented in the backend yet
 */
export async function revokeShare(shareId: string): Promise<void> {
  return apiClient.delete<void>(`/shares/${shareId}`);
}

// Public Share Access (No Authentication)

/**
 * Get information about a public share
 * GET /api/public/share/{token}/info
 * This endpoint does not require authentication
 */
export async function getPublicShareInfo(token: string): Promise<ShareInfo> {
  // Use request directly without automatic auth header
  const API_URL =
    import.meta.env.VITE_API_URL || "http://localhost:8080/api/v1";
  const response = await fetch(`${API_URL}/public/share/${token}/info`);

  if (!response.ok) {
    let errorMessage = "Failed to get share info";
    try {
      const errorData = await response.json();
      errorMessage = errorData.error || errorData.message || errorMessage;
    } catch {
      // If parsing fails, use status-based messages
      if (response.status === 404) {
        errorMessage = "Share not found";
      } else if (response.status === 410) {
        errorMessage = "Share has expired";
      } else {
        errorMessage = response.statusText || errorMessage;
      }
    }
    const error = new Error(errorMessage) as Error & { status?: number };
    error.status = response.status;
    throw error;
  }

  return response.json();
}

/**
 * Create a session for accessing a password-protected share
 * POST /api/public/share/{token}/session
 * This endpoint does not require authentication
 */
export async function createShareSession(
  token: string,
  request: ShareSessionRequest,
): Promise<ShareSessionResponse> {
  // Use request directly without automatic auth header
  const API_URL =
    import.meta.env.VITE_API_URL || "http://localhost:8080/api/v1";
  const response = await fetch(`${API_URL}/public/share/${token}/session`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(request),
  });

  if (!response.ok) {
    let errorMessage = "Failed to create share session";
    try {
      const errorData = await response.json();
      errorMessage = errorData.error || errorData.message || errorMessage;
    } catch {
      errorMessage = response.statusText || errorMessage;
    }
    throw new Error(errorMessage);
  }

  return response.json();
}

/**
 * Download a file from a public share
 * GET /api/public/share/{token}/file
 * Requires a valid session token (from createShareSession)
 */
export async function downloadPublicShareFile(
  token: string,
  sessionToken: string,
): Promise<Blob> {
  // Use request with session token in Authorization header
  const API_URL =
    import.meta.env.VITE_API_URL || "http://localhost:8080/api/v1";
  const response = await fetch(`${API_URL}/public/share/${token}/file`, {
    headers: {
      Authorization: `Bearer ${sessionToken}`,
    },
  });

  if (!response.ok) {
    let errorMessage = "Failed to download file";
    try {
      const errorData = await response.json();
      errorMessage = errorData.error || errorData.message || errorMessage;
    } catch {
      errorMessage = response.statusText || errorMessage;
    }
    throw new Error(errorMessage);
  }

  return response.blob();
}

/**
 * Helper function to trigger file download in the browser
 */
export function triggerFileDownload(blob: Blob, fileName: string): void {
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  window.URL.revokeObjectURL(url);
  document.body.removeChild(a);
}
