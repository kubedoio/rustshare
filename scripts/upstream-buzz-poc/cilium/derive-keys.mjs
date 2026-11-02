// Derive disposable public keys and NIP-19 test credentials without printing
// the input keys. Output files inherit the caller's mode-0600 temp directory.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const [packagePath, inputPath, publicPath, credentialPath] = process.argv.slice(2);
if (![packagePath, inputPath, publicPath, credentialPath].every(Boolean)) {
	throw new Error('usage: derive-keys.mjs package.json private.json public.json credentials.env');
}
const require = createRequire(resolve(packagePath));
const { getPublicKey, nip19 } = require('nostr-tools');
const keys = JSON.parse(readFileSync(inputPath, 'utf8'));
const publicMetadata = {};
const credentials = ['BUZZ_TEST_DISPOSABLE_CONFIRM=DISPOSABLE_ONLY'];
for (const tenant of ['a', 'b']) {
	const suffix = tenant.toUpperCase();
	for (const role of ['owner', 'operator', 'relay', 'member', 'friend']) {
		const name = `${role}_${tenant}`;
		const secret = Buffer.from(keys[name], 'hex');
		if (secret.length !== 32) throw new Error(`invalid_test_key_${name}`);
		const pubkey = getPublicKey(secret);
		publicMetadata[`${name}_pubkey`] = pubkey;
		if (role === 'owner') publicMetadata[`owner_${tenant}_pubkey`] = pubkey;
		if (role === 'operator') publicMetadata[`operator_${tenant}_pubkey`] = pubkey;
		if (role === 'member') {
			publicMetadata[`member_${tenant}_pubkey`] = pubkey;
			credentials.push(`BUZZ_TEST_${tenant === 'a' ? '' : 'B_'}MEMBER_NSEC=${nip19.nsecEncode(secret)}`);
		}
		if (role === 'operator') publicMetadata[`operator_${suffix}_pubkey`] = pubkey;
	}
}
writeFileSync(publicPath, `${JSON.stringify(publicMetadata)}\n`, { mode: 0o600, flag: 'wx' });
writeFileSync(credentialPath, `${credentials.join('\n')}\n`, { mode: 0o600, flag: 'wx' });
