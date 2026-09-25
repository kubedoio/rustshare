// Container-only bootstrap for the bundled Buzz deployment.
// It writes a mode-0600 env file and never prints secret values.
import { randomBytes } from 'node:crypto';
import { chmodSync, mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { schnorr } from '@noble/curves/secp256k1.js';
import { bytesToHex, hexToBytes } from '@noble/curves/utils.js';

const output = process.env.ELEMBRA_CHAT_ENV_FILE || '/out/chat.env';
const rotate = process.env.ELEMBRA_CHAT_ROTATE === 'true';
const hex64 = /^[0-9a-f]{64}$/;

const value = (name) => process.env[name]?.trim() || '';
const publicKey = (secret, name) => {
	if (!hex64.test(secret)) throw new Error(`${name} must be 64 lowercase hexadecimal characters`);
	try {
		return bytesToHex(schnorr.getPublicKey(hexToBytes(secret)));
	} catch {
		throw new Error(`${name} is not a valid Schnorr scalar`);
	}
};
const newSecret = () => bytesToHex(randomBytes(32));

function chooseSecret(primary, alias, label) {
	if (primary && alias && primary !== alias) throw new Error(`${label} key aliases do not match`);
	return primary || alias || newSecret();
}

const serviceSk = rotate
	? newSecret()
	: chooseSecret(value('BUZZ_SERVICE_SK'), value('RUSTSHARE_CHAT_BRIDGE_SECRET_KEY'), 'service');
const relaySk = rotate ? newSecret() : value('BUZZ_RELAY_PRIVATE_KEY') || newSecret();
const servicePk = publicKey(serviceSk, 'BUZZ_SERVICE_SK');
const relayPk = publicKey(relaySk, 'BUZZ_RELAY_PRIVATE_KEY');

if (value('BUZZ_RELAY_OWNER_PUBKEY') && value('BUZZ_RELAY_OWNER_PUBKEY') !== servicePk) {
	throw new Error('BUZZ_RELAY_OWNER_PUBKEY does not match the service key');
}
if (value('BUZZ_RELAY_PUBKEY') && value('BUZZ_RELAY_PUBKEY') !== relayPk) {
	throw new Error('BUZZ_RELAY_PUBKEY does not match BUZZ_RELAY_PRIVATE_KEY');
}

const relayWs = value('BUZZ_RELAY_WS') || 'ws://localhost:7447';
const lines = [
	`BUZZ_SERVICE_SK=${serviceSk}`,
	`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY=${serviceSk}`,
	`BUZZ_RELAY_OWNER_PUBKEY=${servicePk}`,
	`BUZZ_RELAY_PRIVATE_KEY=${relaySk}`,
	`BUZZ_RELAY_PUBKEY=${relayPk}`,
	`BUZZ_RELAY_WS=${relayWs}`,
	`BUZZ_RELAY_URL=${value('BUZZ_RELAY_URL') || relayWs}`,
	`RUSTSHARE_CHAT_AUTHORITY=${value('RUSTSHARE_CHAT_AUTHORITY') || 'buzz'}`,
	`RUSTSHARE_CHAT_PROVISIONING=${value('RUSTSHARE_CHAT_PROVISIONING') || 'auto'}`,
	`RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL=${value('RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL') || relayWs}`,
	`RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL=${value('RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL') || relayWs}`
];

mkdirSync(dirname(output), { recursive: true, mode: 0o700 });
const temporary = `${output}.tmp-${process.pid}`;
writeFileSync(temporary, `${lines.join('\n')}\n`, { mode: 0o600 });
chmodSync(temporary, 0o600);
renameSync(temporary, output);
console.log(`Chat identities are ready in ${output} (private values were not printed)`);
