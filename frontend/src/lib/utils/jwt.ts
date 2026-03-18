import type { User } from '../api/types';

interface JWTPayload {
  sub: string;  // user ID
  email: string;
  display_name: string;
  is_admin: boolean;
  exp: number;
  iat: number;
}

export function decodeJWT(token: string): User | null {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return null;

    const payload = JSON.parse(atob(parts[1]));
    return {
      id: payload.sub,
      email: payload.email,
      display_name: payload.display_name,
      is_admin: payload.is_admin || false
    };
  } catch (error) {
    console.error('Failed to decode JWT:', error);
    return null;
  }
}

export function isTokenExpired(token: string): boolean {
  try {
    const parts = token.split('.');
    if (parts.length !== 3) return true;

    const payload: JWTPayload = JSON.parse(atob(parts[1]));
    return Date.now() >= payload.exp * 1000;
  } catch {
    return true;
  }
}
