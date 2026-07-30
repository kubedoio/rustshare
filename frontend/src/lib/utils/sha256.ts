/**
 * Compute the lowercase hex SHA-256 of a string using Web Crypto.
 *
 * Returns null when `crypto.subtle` is unavailable (e.g. non-secure contexts),
 * so callers can fall back to a safe default instead of throwing.
 */
export async function sha256Hex(text: string): Promise<string | null> {
	const subtle = globalThis.crypto?.subtle;
	if (!subtle) return null;
	const digest = await subtle.digest('SHA-256', new TextEncoder().encode(text));
	return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}
