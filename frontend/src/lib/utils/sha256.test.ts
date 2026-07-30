import { describe, it, expect, vi, afterEach } from 'vitest';
import { webcrypto } from 'node:crypto';
import { sha256Hex } from './sha256';

describe('sha256Hex', () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('computes the lowercase hex SHA-256 of a string', async () => {
		vi.stubGlobal('crypto', webcrypto);
		await expect(sha256Hex('hello')).resolves.toBe(
			'2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824'
		);
	});

	it('computes the SHA-256 of an empty string', async () => {
		vi.stubGlobal('crypto', webcrypto);
		await expect(sha256Hex('')).resolves.toBe(
			'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
		);
	});

	it('hashes UTF-8 encoded content', async () => {
		vi.stubGlobal('crypto', webcrypto);
		// printf 'héllo' | sha256sum
		await expect(sha256Hex('héllo')).resolves.toBe(
			'3c48591d8d098a4538f5e013dfcf406e948eac4d3277b10bf614e295d6068179'
		);
	});

	it('returns null when Web Crypto is unavailable', async () => {
		vi.stubGlobal('crypto', undefined);
		await expect(sha256Hex('hello')).resolves.toBeNull();
	});
});
