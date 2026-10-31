// Disposable, wire-level conformance probe. Private keys enter only via env and optional 0600 GUI file.
import { createRequire } from 'node:module';
import { writeFile } from 'node:fs/promises';
import { randomBytes, randomUUID } from 'node:crypto';

const require = createRequire(new URL('../../frontend/package.json', import.meta.url));
const { schnorr } = require('@noble/curves/secp256k1.js');
const { bytesToHex, hexToBytes } = require('@noble/curves/utils.js');
const url = process.env.POC_RELAY_URL;
const keys = Object.fromEntries(['OWNER', 'ALICE', 'BOB', 'OUTSIDER'].map((name) => [name, process.env[`POC_${name}_SK`]]));
if (!url || Object.values(keys).some((key) => !/^[0-9a-f]{64}$/i.test(key || ''))) {
	console.error('POC_RELAY_URL and four disposable POC_*_SK hex keys are required');
	process.exit(2);
}

const pubkey = (key) => bytesToHex(schnorr.getPublicKey(hexToBytes(key)));
const pubs = Object.fromEntries(Object.entries(keys).map(([name, key]) => [name, pubkey(key)]));
function nsec(key) {
	const alphabet = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
	let bits = 0, value = 0;
	const words = [];
	for (const byte of hexToBytes(key)) {
		value = (value << 8) | byte; bits += 8;
		while (bits >= 5) { bits -= 5; words.push((value >> bits) & 31); }
	}
	if (bits) words.push((value << (5 - bits)) & 31);
	const polymod = (values) => {
		let checksum = 1;
		for (const v of values) {
			const top = checksum >>> 25;
			checksum = ((checksum & 0x1ffffff) << 5) ^ v;
			for (const [bit, generator] of [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3].entries()) if ((top >>> bit) & 1) checksum ^= generator;
		}
		return checksum;
	};
	const expanded = [...'nsec'].map((c) => c.charCodeAt(0) >> 5).concat(0, ...[...'nsec'].map((c) => c.charCodeAt(0) & 31));
	const remainder = polymod([...expanded, ...words, 0, 0, 0, 0, 0, 0]) ^ 1;
	const checksum = Array.from({ length: 6 }, (_, i) => (remainder >>> (5 * (5 - i))) & 31);
	return `nsec1${[...words, ...checksum].map((word) => alphabet[word]).join('')}`;
}
const hash = async (value) => bytesToHex(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))));
async function event(key, kind, tags = [], content = '', createdAt = Math.floor(Date.now() / 1000)) {
	const base = { pubkey: pubkey(key), created_at: createdAt, kind, tags, content };
	const id = await hash(JSON.stringify([0, base.pubkey, base.created_at, kind, tags, content]));
	return { ...base, id, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(key))) };
}

class Session {
	constructor(relay) {
		this.relay = relay;
		this.frames = [];
		this.waiters = [];
		this.socket = new WebSocket(relay);
		this.socket.addEventListener('message', ({ data }) => {
			try { this.push(JSON.parse(String(data))); } catch { this.push(['INVALID']); }
		});
		this.socket.addEventListener('close', () => this.push(['CLOSE']));
		this.socket.addEventListener('error', () => this.push(['TRANSPORT_ERROR']));
	}
	push(frame) {
		const i = this.waiters.findIndex(({ predicate }) => predicate(frame));
		if (i < 0) this.frames.push(frame);
		else this.waiters.splice(i, 1)[0].resolve(frame);
	}
	next(predicate, ms = 8000) {
		const i = this.frames.findIndex(predicate);
		if (i >= 0) return Promise.resolve(this.frames.splice(i, 1)[0]);
		return new Promise((resolve) => {
			const waiter = { predicate, resolve: (frame) => { clearTimeout(timer); resolve(frame); } };
			const timer = setTimeout(() => {
				this.waiters = this.waiters.filter((item) => item !== waiter);
				resolve(['TIMEOUT']);
			}, ms);
			this.waiters.push(waiter);
		});
	}
	async auth(key) {
		const frame = await this.next((f) => ['AUTH', 'CLOSE', 'TRANSPORT_ERROR'].includes(f[0]));
		if (frame[0] !== 'AUTH') return { accepted: false, outcome: frame[0], challenged: false };
		const proof = await event(key, 22242, [['relay', this.relay], ['challenge', frame[1]]]);
		this.socket.send(JSON.stringify(['AUTH', proof]));
		const answer = await this.next((f) => (f[0] === 'OK' && f[1] === proof.id) || ['CLOSE', 'TRANSPORT_ERROR'].includes(f[0]));
		return { accepted: answer[0] === 'OK' && answer[2] === true, outcome: answer[0] === 'OK' ? 'OK' : answer[0], challenged: true };
	}
	async publish(signed) {
		this.socket.send(JSON.stringify(['EVENT', signed]));
		const answer = await this.next((f) => (f[0] === 'OK' && f[1] === signed.id) || ['CLOSE', 'TRANSPORT_ERROR'].includes(f[0]));
		return { accepted: answer[0] === 'OK' && answer[2] === true, outcome: answer[0] === 'OK' ? 'OK' : answer[0], event_id: signed.id };
	}
	async read(filter) {
		const sub = randomUUID();
		this.socket.send(JSON.stringify(['REQ', sub, filter]));
		const events = [];
		let outcome = 'TIMEOUT';
		for (;;) {
			const frame = await this.next((f) => (['EVENT', 'EOSE', 'CLOSED'].includes(f[0]) && f[1] === sub) || ['CLOSE', 'TRANSPORT_ERROR', 'TIMEOUT'].includes(f[0]));
			if (frame[0] === 'EVENT') events.push({ id: frame[2]?.id, kind: frame[2]?.kind, pubkey: frame[2]?.pubkey });
			else { outcome = frame[0]; break; }
		}
		if (this.socket.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(['CLOSE', sub]));
		return { outcome, events };
	}
	async subscribe(filter) {
		const sub = randomUUID();
		this.socket.send(JSON.stringify(['REQ', sub, filter]));
		const first = await this.next((f) => (['EOSE', 'CLOSED'].includes(f[0]) && f[1] === sub) || ['CLOSE', 'TRANSPORT_ERROR', 'TIMEOUT'].includes(f[0]));
		return { sub, outcome: first[0] };
	}
	close() { this.socket.close(); }
}

const results = [];
const sessions = [];
const connect = async (relay, key) => {
	const session = new Session(relay);
	sessions.push(session);
	return { session, auth: await session.auth(key) };
};
async function check(name, fn) {
	try {
		const detail = await fn();
		results.push({ name, ...detail });
	} catch (error) {
		results.push({ name, pass: false, outcome: error?.name || 'error' });
	}
}
const result = (pass, detail = {}) => ({ pass, ...detail });
const explicitAuthDenial = (auth) => auth.challenged && !auth.accepted && ['OK', 'CLOSE'].includes(auth.outcome);
let owner, alice, bob, publicChannel, privateChannel, publicMessage, privateMessage, bobSubscription, aliceAdd, bobRemove;
try {
	if (process.env.POC_GUI_NSEC_FILE) {
		await writeFile(process.env.POC_GUI_NSEC_FILE, `${nsec(keys.ALICE)}\n`, { mode: 0o600, flag: 'wx' });
	}
	await check('owner_nip42', async () => {
		const joined = await connect(url, keys.OWNER); owner = joined.session;
		return result(joined.auth.accepted, joined.auth);
	});
	await check('nonmember_nip42_denied', async () => {
		const joined = await connect(url, keys.OUTSIDER); joined.session.close();
		return result(explicitAuthDenial(joined.auth), joined.auth);
	});
	for (const name of ['ALICE', 'BOB']) {
		await check(`nip43_add_${name.toLowerCase()}`, async () => {
			const command = await event(keys.OWNER, 9030, [['p', pubs[name]], ['role', 'member']]);
			if (name === 'ALICE') aliceAdd = command;
			const wire = await owner.publish(command);
			return result(wire.accepted, wire);
		});
	}
	await check('nip43_add_exact_replay', async () => {
		const wire = await owner.publish(aliceAdd);
		return result(wire.outcome === 'OK', { ...wire, note: 'acceptance is measured, not assumed idempotent' });
	});
	await check('nip43_add_stale_signed_command', async () => {
		const stale = await event(keys.OWNER, 9030, [['p', pubs.ALICE]], '', Math.floor(Date.now() / 1000) - 86400);
		const wire = await owner.publish(stale);
		return result(wire.outcome === 'OK', { ...wire, age_seconds: 86400, note: 'acceptance is measured, not assumed safe' });
	});
	for (const name of ['ALICE', 'BOB']) {
		await check(`member_nip42_${name.toLowerCase()}`, async () => {
			const joined = await connect(url, keys[name]);
			if (name === 'ALICE') alice = joined.session; else bob = joined.session;
			return result(joined.auth.accepted, joined.auth);
		});
	}
	publicChannel = randomUUID(); privateChannel = randomUUID();
	for (const [name, id, visibility] of [['public', publicChannel, 'open'], ['private', privateChannel, 'private']]) {
		await check(`create_${name}_channel`, async () => {
			const wire = await owner.publish(await event(keys.OWNER, 9007, [['h', id], ['name', `POC ${name}`], ['visibility', visibility]]));
			return result(wire.accepted, wire);
		});
	}
	await check('private_channel_add_alice', async () => {
		const wire = await owner.publish(await event(keys.OWNER, 9000, [['h', privateChannel], ['p', pubs.ALICE]]));
		return result(wire.accepted, wire);
	});
	await check('private_channel_alice_message', async () => {
		privateMessage = await event(keys.ALICE, 9, [['h', privateChannel]], `POC ${randomUUID()}`);
		const wire = await alice.publish(privateMessage);
		return result(wire.accepted, wire);
	});
	await check('private_channel_alice_history', async () => {
		const read = await alice.read({ kinds: [9], '#h': [privateChannel], limit: 20 });
		return result(read.outcome === 'EOSE' && read.events.some((item) => item.id === privateMessage.id), read);
	});
	for (const kind of [39000, 39001, 39002]) {
		await check(`channel_discovery_${kind}`, async () => {
			const read = await alice.read({ kinds: [kind], '#d': [publicChannel], limit: 20 });
			return result(read.outcome === 'EOSE' && read.events.some((item) => item.kind === kind), read);
		});
	}
	await check('kind9_publish', async () => {
		publicMessage = await event(keys.ALICE, 9, [['h', publicChannel]], `POC ${randomUUID()}`);
		const wire = await alice.publish(publicMessage);
		return result(wire.accepted, wire);
	});
	await check('kind9_history_bob', async () => {
		const read = await bob.read({ kinds: [9], '#h': [publicChannel], limit: 20 });
		return result(read.outcome === 'EOSE' && read.events.some((item) => item.id === publicMessage.id), read);
	});
	await check('private_channel_bob_denied', async () => {
		const metadata = await bob.read({ kinds: [39000], '#d': [privateChannel], limit: 20 });
		const history = await bob.read({ kinds: [9], '#h': [privateChannel], limit: 20 });
		return result([metadata, history].every((read) => read.outcome === 'CLOSED' || (read.outcome === 'EOSE' && read.events.length === 0)), { metadata, history });
	});
	await check('nip17_giftwrap_routing', async () => {
		const wrap = await event(bytesToHex(randomBytes(32)), 1059, [['p', pubs.BOB]], 'disposable-opaque-payload');
		const published = await alice.publish(wrap);
		const read = await bob.read({ kinds: [1059], '#p': [pubs.BOB], limit: 20 });
		return result(published.accepted && read.events.some((item) => item.id === wrap.id), { published, read, note: 'NIP-17 routing only; payload is not NIP-44 encrypted' });
	});
	results.push({ name: 'nip17_encrypted_dm_and_discovery', pass: null, outcome: 'UNVERIFIED_NO_NIP44_PAYLOAD_OR_DM_CHANNEL_CREATION' });
	await check('open_pre_revocation_subscription', async () => {
		bobSubscription = await bob.subscribe({ kinds: [9], '#h': [publicChannel], since: Math.floor(Date.now() / 1000) });
		return result(bobSubscription.outcome === 'EOSE', { outcome: bobSubscription.outcome });
	});
	await check('nip43_remove_bob', async () => {
		bobRemove = await event(keys.OWNER, 9031, [['p', pubs.BOB]]);
		const wire = await owner.publish(bobRemove);
		return result(wire.accepted, wire);
	});
	await check('nip43_remove_exact_replay', async () => {
		const wire = await owner.publish(bobRemove);
		return result(wire.outcome === 'OK', { ...wire, note: 'acceptance is measured, not assumed idempotent' });
	});
	await check('revoked_existing_publish', async () => {
		if (bob.socket.readyState === WebSocket.CLOSED) return result(true, { outcome: 'SOCKET_CLOSED_BEFORE_PUBLISH' });
		const wire = await bob.publish(await event(keys.BOB, 9, [['h', publicChannel]], `POC ${randomUUID()}`));
		return result(!wire.accepted && ['OK', 'CLOSE'].includes(wire.outcome), wire);
	});
	await check('revoked_existing_subscription', async () => {
		const marker = await event(keys.ALICE, 9, [['h', publicChannel]], `POC ${randomUUID()}`);
		const sent = await alice.publish(marker);
		const frame = await bob.next((f) => (f[0] === 'EVENT' && f[1] === bobSubscription?.sub && f[2]?.id === marker.id) || (f[0] === 'CLOSED' && f[1] === bobSubscription?.sub) || ['CLOSE', 'TRANSPORT_ERROR'].includes(f[0]), 2500);
		return result(sent.accepted && ['CLOSED', 'CLOSE', 'TIMEOUT'].includes(frame[0]), { published: sent, subscription_outcome: frame[0], observation_ms: 2500 });
	});
	await check('revoked_existing_new_req', async () => {
		if (bob.socket.readyState === WebSocket.CLOSED) return result(true, { outcome: 'SOCKET_CLOSED_BEFORE_REQ' });
		const read = await bob.read({ kinds: [9], '#h': [publicChannel], limit: 20 });
		return result(read.outcome === 'CLOSED' || (read.outcome === 'EOSE' && read.events.length === 0), read);
	});
	await check('revoked_fresh_connection', async () => {
		const joined = await connect(url, keys.BOB);
		const read = joined.auth.accepted ? await joined.session.read({ kinds: [9], '#h': [publicChannel], limit: 20 }) : null;
		return result(explicitAuthDenial(joined.auth) || (joined.auth.accepted && read?.outcome === 'CLOSED'), { auth: joined.auth, read });
	});
	if (process.env.POC_RELAY_B_URL && process.env.POC_B_OWNER_SK && process.env.POC_B_USER_SK) {
		await check('tenant_b_member_nip42', async () => {
			const joined = await connect(process.env.POC_RELAY_B_URL, process.env.POC_B_OWNER_SK);
			const ownerB = joined.session;
			if (!joined.auth.accepted) return result(false, joined.auth);
			const add = await ownerB.publish(await event(process.env.POC_B_OWNER_SK, 9030, [['p', pubkey(process.env.POC_B_USER_SK)]]));
			const member = await connect(process.env.POC_RELAY_B_URL, process.env.POC_B_USER_SK);
			return result(add.accepted && member.auth.accepted, { owner_auth: joined.auth, add, member_auth: member.auth });
		});
		await check('cross_tenant_alice_denied', async () => {
			const joined = await connect(process.env.POC_RELAY_B_URL, keys.ALICE);
			return result(explicitAuthDenial(joined.auth), joined.auth);
		});
		await check('cross_tenant_b_user_denied', async () => {
			const joined = await connect(url, process.env.POC_B_USER_SK);
			return result(explicitAuthDenial(joined.auth), joined.auth);
		});
	} else for (const name of ['tenant_b_member_nip42', 'cross_tenant_alice_denied', 'cross_tenant_b_user_denied']) results.push({ name, pass: null, outcome: 'UNVERIFIED_NO_SECOND_RELAY' });
	if (process.env.POC_WRONG_HOST_URL) {
		await check('wrong_host_denied', async () => {
			const joined = await connect(process.env.POC_WRONG_HOST_URL, keys.ALICE);
			return result(joined.auth.accepted ? false : explicitAuthDenial(joined.auth) ? true : null, joined.auth);
		});
	} else results.push({ name: 'wrong_host_denied', pass: null, outcome: 'UNVERIFIED_NO_HOST_ROUTE' });
} finally {
	for (const session of sessions) session.close();
	console.log(JSON.stringify({ relay: url, public_keys: pubs, public_channel: publicChannel, private_channel: privateChannel, public_message_id: publicMessage?.id, private_message_id: privateMessage?.id, results }, null, 2));
}
if (results.some((item) => item.pass === false)) process.exitCode = 1;
