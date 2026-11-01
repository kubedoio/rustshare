// Disposable, wire-level conformance probe. Private keys enter only via env and optional 0600 GUI file.
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { randomBytes, randomUUID } from 'node:crypto';
import { lookup as dnsLookup } from 'node:dns';
import { createInterface } from 'node:readline';

if (!process.env.POC_TEST_PACKAGE) throw new Error('POC_TEST_PACKAGE is required');
const require = createRequire(process.env.POC_TEST_PACKAGE);
const WebSocket = require('ws');
const { schnorr } = require('@noble/curves/secp256k1.js');
const { bytesToHex, hexToBytes } = require('@noble/curves/utils.js');
const { finalizeEvent, generateSecretKey, getPublicKey, nip19, nip44, nip59, verifyEvent } = require('nostr-tools');
const testLookup = (hostname, options, callback) => {
	if (hostname.endsWith('.poc.invalid')) {
		if (options?.all) callback(null, [{ address: '127.0.0.1', family: 4 }]);
		else callback(null, '127.0.0.1', 4);
		return;
	}
	dnsLookup(hostname, options, callback);
};
const url = process.env.POC_RELAY_URL;
const keys = Object.fromEntries(['OWNER', 'ALICE', 'BOB', 'OUTSIDER'].map((name) => [name, process.env[`POC_${name}_SK`]]));
if (!url || Object.values(keys).some((key) => !/^[0-9a-f]{64}$/i.test(key || ''))) {
	console.error('POC_RELAY_URL and four disposable POC_*_SK hex keys are required');
	process.exit(2);
}

const pubkey = (key) => bytesToHex(schnorr.getPublicKey(hexToBytes(key)));
const pubs = Object.fromEntries(Object.entries(keys).map(([name, key]) => [name, pubkey(key)]));
function nsec(key) {
	return nip19.nsecEncode(hexToBytes(key));
}
const hash = async (value) => bytesToHex(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))));
async function event(key, kind, tags = [], content = '', createdAt = Math.floor(Date.now() / 1000)) {
	const base = { pubkey: pubkey(key), created_at: createdAt, kind, tags, content };
	const id = await hash(JSON.stringify([0, base.pubkey, base.created_at, kind, tags, content]));
	return { ...base, id, sig: bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(key))) };
}
function nip17Wrap(plaintext, createdAt) {
	const senderSecret = hexToBytes(keys.ALICE);
	const recipientPubkey = pubs.BOB;
	const rumor = nip59.createRumor({ kind: 14, created_at: createdAt, tags: [['p', recipientPubkey]], content: plaintext }, senderSecret);
	const sealKey = nip44.v2.utils.getConversationKey(senderSecret, recipientPubkey);
	const seal = finalizeEvent({ kind: 13, created_at: createdAt, tags: [], content: nip44.v2.encrypt(JSON.stringify(rumor), sealKey) }, senderSecret);
	const ephemeral = generateSecretKey();
	const wrapKey = nip44.v2.utils.getConversationKey(ephemeral, recipientPubkey);
	const wrap = finalizeEvent({ kind: 1059, created_at: createdAt, tags: [['p', recipientPubkey]], content: nip44.v2.encrypt(JSON.stringify(seal), wrapKey) }, ephemeral);
	return { rumor, seal, wrap };
}

class Session {
	constructor(relay) {
		this.relay = relay;
		this.frames = [];
		this.waiters = [];
		this.observations = [];
		this.socket = new WebSocket(relay, { lookup: testLookup });
		this.websocket_open = false;
		this.http_status = null;
		this.transport_error_code = null;
		this.socket.on('open', () => { this.websocket_open = true; });
		this.socket.addEventListener('message', ({ data }) => {
			try { this.push(JSON.parse(String(data))); } catch { this.push(['INVALID']); }
		});
		this.socket.addEventListener('close', (frame) => this.push(['SOCKET_CLOSE', frame.code]));
		this.socket.on('error', (error) => { this.transport_error_code = error.code || 'ERROR'; this.push(['TRANSPORT_ERROR', this.transport_error_code]); });
		this.socket.on('unexpected-response', (_request, response) => { this.http_status = response.statusCode; this.push(['HTTP_ERROR', response.statusCode]); });
	}
	push(frame) {
		const observed = { type: frame[0] };
		if (frame[0] === 'EVENT' && frame[2]) Object.assign(observed, { subscription: frame[1], event_id: frame[2].id, kind: frame[2].kind, pubkey: frame[2].pubkey });
		else if (frame[0] === 'CLOSED' || frame[0] === 'CLOSE') observed.subscription = frame[1];
		else if (frame[0] === 'OK') Object.assign(observed, { event_id: frame[1], accepted: frame[2] });
		else if (frame[0] === 'SOCKET_CLOSE') observed.code = frame[1];
		else if (frame[0] === 'HTTP_ERROR') observed.status = frame[1];
		this.observations.push(observed);
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
		const frame = await this.next((f) => ['AUTH', 'SOCKET_CLOSE', 'HTTP_ERROR', 'TRANSPORT_ERROR'].includes(f[0]));
		if (frame[0] !== 'AUTH') return { accepted: false, outcome: frame[0], challenged: false, websocket_open: this.websocket_open, http_status: this.http_status, transport_error_code: this.transport_error_code };
		const proof = await event(key, 22242, [['relay', this.relay], ['challenge', frame[1]]]);
		this.socket.send(JSON.stringify(['AUTH', proof]));
		const answer = await this.next((f) => (f[0] === 'OK' && f[1] === proof.id) || ['SOCKET_CLOSE', 'HTTP_ERROR', 'TRANSPORT_ERROR'].includes(f[0]));
		return { accepted: answer[0] === 'OK' && answer[2] === true, outcome: answer[0] === 'OK' ? 'OK' : answer[0], challenged: true, websocket_open: this.websocket_open, http_status: this.http_status };
	}
	async publish(signed) {
		this.socket.send(JSON.stringify(['EVENT', signed]));
		const answer = await this.next((f) => (f[0] === 'OK' && f[1] === signed.id) || ['SOCKET_CLOSE', 'HTTP_ERROR', 'TRANSPORT_ERROR'].includes(f[0]));
		return { accepted: answer[0] === 'OK' && answer[2] === true, outcome: answer[0] === 'OK' ? 'OK' : answer[0], event_id: signed.id, ...(answer[3] ? { relay_message: answer[3] } : {}) };
	}
	async read(filter, includePayload = false) {
		const sub = randomUUID();
		this.socket.send(JSON.stringify(['REQ', sub, filter]));
		const events = [];
		let outcome = 'TIMEOUT';
		for (;;) {
			const frame = await this.next((f) => (['EVENT', 'EOSE', 'CLOSED'].includes(f[0]) && f[1] === sub) || ['SOCKET_CLOSE', 'TRANSPORT_ERROR', 'TIMEOUT'].includes(f[0]));
			if (frame[0] === 'EVENT') events.push({ id: frame[2]?.id, kind: frame[2]?.kind, pubkey: frame[2]?.pubkey, ...(includePayload ? { content: frame[2]?.content, tags: frame[2]?.tags, created_at: frame[2]?.created_at, sig: frame[2]?.sig } : {}) });
			else { outcome = frame[0]; break; }
		}
		if (this.socket.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(['CLOSE', sub]));
		return { outcome, events };
	}
	async subscribe(filter) {
		const sub = randomUUID();
		this.socket.send(JSON.stringify(['REQ', sub, filter]));
		const first = await this.next((f) => (['EOSE', 'CLOSED'].includes(f[0]) && f[1] === sub) || ['SOCKET_CLOSE', 'TRANSPORT_ERROR', 'TIMEOUT'].includes(f[0]));
		return { sub, outcome: first[0] };
	}
	close() { this.socket.terminate(); }
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
const deniedWithoutEvents = (read) => ['CLOSED', 'EOSE'].includes(read.outcome) && read.events.length === 0;
const hasChannelTag = (event, channelId) => event.tags?.some((tag) => tag[0] === 'd' && tag[1] === channelId) === true;
const hasChannelRef = (event, channelId) => event.tags?.some((tag) => tag[0] === 'h' && tag[1] === channelId) === true;
assert.equal(deniedWithoutEvents({ outcome: 'CLOSED', events: [{ kind: 9 }] }), false);
assert.equal(deniedWithoutEvents({ outcome: 'EOSE', events: [{ kind: 9 }] }), false);
assert.equal(deniedWithoutEvents({ outcome: 'CLOSED', events: [] }), true);
assert.equal(deniedWithoutEvents({ outcome: 'EOSE', events: [] }), true);
assert.equal(hasChannelTag({ tags: [['d', 'other-channel']] }, 'requested-channel'), false);
assert.equal(hasChannelTag({ tags: [['d', 'requested-channel']] }, 'requested-channel'), true);
const explicitAuthDenial = (auth) => auth.challenged && !auth.accepted && ['OK', 'SOCKET_CLOSE'].includes(auth.outcome);
async function desktopHold() {
	console.log(`desktop_test_ready alice_pubkey=${pubs.ALICE} bob_pubkey=${pubs.BOB} relay=${url} channel=${publicChannel} dm_channel=${dmChannel} gui_nsec_path=${process.env.POC_GUI_NSEC_FILE}`);
	console.log('desktop_test_commands: SEND <text> | WAIT <text> | HISTORY <event_id> | SEND_DM <text> | WAIT_DM <text> | HISTORY_DM <event_id> | EXIT');
	for await (const line of createInterface({ input: process.stdin })) {
		const [command, ...parts] = line.trim().split(/\s+/);
		const value = parts.join(' ');
		if (command === 'EXIT') break;
		if (command === 'SEND' && value) {
			const signed = await event(keys.ALICE, 9, [['h', publicChannel]], value);
			const sent = await alice.publish(signed);
			console.log(JSON.stringify({ name: 'relay_to_desktop', event_id: signed.id, pubkey: pubs.ALICE, accepted: sent.accepted }));
		} else if (command === 'WAIT' && value) {
			let found;
			for (let attempt = 0; attempt < 45 && !found; attempt++) {
				found = (await bob.read({ kinds: [9], '#h': [publicChannel], limit: 200 }, true)).events.find((item) => item.content === value && item.pubkey === pubs.ALICE && hasChannelRef(item, publicChannel));
				if (!found) await new Promise((resolve) => setTimeout(resolve, 1000));
			}
			console.log(JSON.stringify({ name: 'desktop_to_relay', found: Boolean(found), event_id: found?.id, channel_id: found?.tags?.find((tag) => tag[0] === 'h')?.[1], pubkey: found?.pubkey, alice_pubkey_match: found?.pubkey === pubs.ALICE, kind: found?.kind }));
		} else if (command === 'HISTORY' && value) {
			const found = (await bob.read({ kinds: [9], '#h': [publicChannel], limit: 200 }, true)).events.find((item) => item.id === value && item.pubkey === pubs.ALICE && hasChannelRef(item, publicChannel));
			console.log(JSON.stringify({ name: 'desktop_restart_history', found: Boolean(found), event_id: found?.id, channel_id: found?.tags?.find((tag) => tag[0] === 'h')?.[1], pubkey: found?.pubkey, alice_pubkey_match: found?.pubkey === pubs.ALICE, kind: found?.kind }));
		} else if (command === 'SEND_DM' && value) {
			const signed = await event(keys.BOB, 9, [['h', dmChannel]], value);
			const sent = await bob.publish(signed);
			console.log(JSON.stringify({ name: 'protocol_to_desktop_dm', channel_id: dmChannel, event_id: signed.id, pubkey: pubs.BOB, accepted: sent.accepted }));
		} else if (command === 'WAIT_DM' && value) {
			let found;
			for (let attempt = 0; attempt < 45 && !found; attempt++) {
				found = (await bob.read({ kinds: [9], '#h': [dmChannel], limit: 200 }, true)).events.find((item) => item.content === value && item.pubkey === pubs.ALICE && hasChannelRef(item, dmChannel));
				if (!found) await new Promise((resolve) => setTimeout(resolve, 1000));
			}
			console.log(JSON.stringify({ name: 'desktop_to_protocol_dm', found: Boolean(found), channel_id: found?.tags?.find((tag) => tag[0] === 'h')?.[1], event_id: found?.id, pubkey: found?.pubkey, alice_pubkey_match: found?.pubkey === pubs.ALICE, kind: found?.kind }));
		} else if (command === 'HISTORY_DM' && value) {
			const found = (await bob.read({ kinds: [9], '#h': [dmChannel], limit: 200 }, true)).events.find((item) => item.id === value && item.pubkey === pubs.ALICE && hasChannelRef(item, dmChannel));
			console.log(JSON.stringify({ name: 'protocol_dm_history', found: Boolean(found), channel_id: found?.tags?.find((tag) => tag[0] === 'h')?.[1], event_id: found?.id, pubkey: found?.pubkey, alice_pubkey_match: found?.pubkey === pubs.ALICE, kind: found?.kind }));
		} else console.log(JSON.stringify({ name: 'desktop_test_command_error', reason: 'unknown command' }));
	}
}
let owner, alice, bob, outsider, publicChannel, privateChannel, dmChannel, publicMessage, privateMessage, bobSubscriptions = [], aliceAdd, bobRemove;
let aliceNsec;
try {
	await check('nsec_gui_export', async () => {
		aliceNsec = nsec(keys.ALICE);
		return result(aliceNsec.startsWith('nsec1'));
	});
	if (!aliceNsec) throw new Error('NIP-19 key export failed');
	if (process.env.POC_GUI_NSEC_FILE) {
		await writeFile(process.env.POC_GUI_NSEC_FILE, `${aliceNsec}\n`, { mode: 0o600, flag: 'wx' });
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
		return result(null, { ...wire, note: 'replay behavior is recorded, not asserted' });
	});
	await check('nip43_add_stale_signed_command', async () => {
		const stale = await event(keys.OWNER, 9030, [['p', pubs.OUTSIDER]], '', Math.floor(Date.now() / 1000) - 86400);
		const wire = await owner.publish(stale);
		return result(wire.outcome === 'OK' && !wire.accepted, { ...wire, age_seconds: 86400 });
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
			const read = await alice.read({ kinds: [kind], '#d': [publicChannel], limit: 20 }, true);
			const matchingChannelEvents = read.events.filter((item) => item.kind === kind && hasChannelTag(item, publicChannel));
			return result(read.outcome === 'EOSE' && matchingChannelEvents.length > 0, { outcome: read.outcome, event_count: read.events.length, matching_channel_event_count: matchingChannelEvents.length });
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
		return result([metadata, history].every(deniedWithoutEvents), { metadata, history });
	});
	await check('private_channel_add_bob', async () => {
		const wire = await owner.publish(await event(keys.OWNER, 9000, [['h', privateChannel], ['p', pubs.BOB]]));
		return result(wire.accepted, wire);
	});
	await check('private_channel_bob_member_history', async () => {
		const read = await bob.read({ kinds: [9], '#h': [privateChannel], limit: 20 });
		return result(read.outcome === 'EOSE' && read.events.some((item) => item.id === privateMessage.id), read);
	});
	await check('nip43_add_outsider_for_private_dm_denial', async () => {
		const wire = await owner.publish(await event(keys.OWNER, 9030, [['p', pubs.OUTSIDER]]));
		return result(wire.accepted, wire);
	});
	await check('member_nip42_outsider', async () => {
		const joined = await connect(url, keys.OUTSIDER);
		outsider = joined.session;
		return result(joined.auth.accepted, joined.auth);
	});
	await check('nip29_dm_open_and_discovery', async () => {
		const wire = await alice.publish(await event(keys.ALICE, 41010, [['p', pubs.BOB]]));
		const response = wire.relay_message?.startsWith('response:') ? JSON.parse(wire.relay_message.slice(9)) : null;
		dmChannel = response?.channel_id;
		if (!wire.accepted || !/^[0-9a-f-]{36}$/.test(dmChannel || '')) return result(false, { accepted: wire.accepted, outcome: wire.outcome });
		const kinds = [39000, 39001, 39002];
		const discoveries = [];
		for (const kind of kinds) discoveries.push(await bob.read({ kinds: [kind], '#d': [dmChannel], limit: 20 }, true));
		const metadata = discoveries[0].events.find((item) => item.kind === 39000 && hasChannelTag(item, dmChannel));
		const hidden = metadata?.tags?.some((tag) => tag[0] === 'hidden');
		const matchingChannelEvents = discoveries.map((read, index) => read.events.filter((item) => item.kind === kinds[index] && hasChannelTag(item, dmChannel)).length);
		const discovered = discoveries.every((read, index) => read.outcome === 'EOSE' && matchingChannelEvents[index] > 0);
		const dmMessage = await event(keys.ALICE, 9, [['h', dmChannel]], `POC NIP-29 private DM ${randomUUID()}`);
		const dmPublish = await alice.publish(dmMessage);
		const bobDmHistory = await bob.read({ kinds: [9], '#h': [dmChannel], limit: 50 }, true);
		const bobObservedDm = bobDmHistory.events.find((item) => item.id === dmMessage.id);
		const bobSawExactDm = dmPublish.accepted && bobDmHistory.outcome === 'EOSE' && bobObservedDm?.pubkey === pubs.ALICE && hasChannelRef(bobObservedDm, dmChannel);
		const outsiderMetadata = await outsider.read({ kinds: [39000, 39001, 39002], '#d': [dmChannel], limit: 20 });
		const outsiderHistory = await outsider.read({ kinds: [9], '#h': [dmChannel], limit: 20 });
		return result(discovered && hidden === true && bobSawExactDm && deniedWithoutEvents(outsiderMetadata) && deniedWithoutEvents(outsiderHistory), { channel_id: dmChannel, discovery_kinds: discoveries.map((read) => read.events[0]?.kind), matching_channel_event_counts: matchingChannelEvents, hidden, participant_dm_event_id: dmMessage.id, participant_dm_author: pubs.ALICE, participant_dm_channel: dmChannel, bob_observed_dm_event_id: bobObservedDm?.id, bob_observed_dm_author: bobObservedDm?.pubkey, bob_observed_dm_channel: bobObservedDm?.tags?.find((tag) => tag[0] === 'h')?.[1], outsider_discovery: outsiderMetadata.outcome, outsider_discovery_event_count: outsiderMetadata.events.length, outsider_history: outsiderHistory.outcome, outsider_history_event_count: outsiderHistory.events.length });
	});
	await check('nip29_dm_bidirectional_messages', async () => {
		const aliceMessage = await event(keys.ALICE, 9, [['h', dmChannel]], `POC NIP-29 Alice ${randomUUID()}`);
		const alicePublish = await alice.publish(aliceMessage);
		const bobRead = await bob.read({ kinds: [9], '#h': [dmChannel], limit: 50 }, true);
		const bobObservedAlice = bobRead.events.find((item) => item.id === aliceMessage.id);
		const bobMessage = await event(keys.BOB, 9, [['h', dmChannel]], `POC NIP-29 Bob ${randomUUID()}`);
		const bobPublish = await bob.publish(bobMessage);
		const aliceRead = await alice.read({ kinds: [9], '#h': [dmChannel], limit: 50 }, true);
		const aliceObservedBob = aliceRead.events.find((item) => item.id === bobMessage.id);
		return result(alicePublish.accepted && bobObservedAlice?.pubkey === pubs.ALICE && bobPublish.accepted && aliceObservedBob?.pubkey === pubs.BOB, { alice_event_id: aliceMessage.id, alice_author: pubs.ALICE, bob_observed_alice_event_id: bobObservedAlice?.id, bob_observed_alice_author: bobObservedAlice?.pubkey, bob_event_id: bobMessage.id, bob_author: pubs.BOB, alice_observed_bob_event_id: aliceObservedBob?.id, alice_observed_bob_author: aliceObservedBob?.pubkey });
	});
	await check('nip17_randomized_timestamp_relay_compatibility', async () => {
		const now = Math.floor(Date.now() / 1000);
		const timestamp = now - 3600 - Math.floor(Math.random() * 3600);
		const plaintext = `POC NIP-17 standard timestamp ${randomUUID()}`;
		const { seal, wrap } = nip17Wrap(plaintext, timestamp);
		const published = await alice.publish(wrap);
		const ageSeconds = now - wrap.created_at;
		return result(published.accepted, { published, outer_signature_valid: verifyEvent(wrap), seal_signature_valid: verifyEvent(seal), randomized_timestamp_age_seconds: ageSeconds, recommended_nip59_range_seconds: 172800, relay_accepted: published.accepted, relay_message: published.relay_message });
	});
	await check('nip17_recent_timestamp_delivery_and_decryption', async () => {
		const bobSecret = hexToBytes(keys.BOB);
		const outsiderSecret = hexToBytes(keys.OUTSIDER);
		const plaintext = `POC NIP-17 recent timestamp ${randomUUID()}`;
		const { seal, wrap } = nip17Wrap(plaintext, Math.floor(Date.now() / 1000));
		const published = await alice.publish(wrap);
		const recipientRead = await bob.read({ kinds: [1059], '#p': [pubs.BOB], limit: 50 }, true);
		const received = recipientRead.events.find((item) => item.id === wrap.id);
		const unwrapped = received ? nip59.unwrapEvent(received, bobSecret) : null;
		let outsiderCannotDecrypt = false;
		try { nip59.unwrapEvent(wrap, outsiderSecret); } catch { outsiderCannotDecrypt = true; }
		return result(published.accepted && verifyEvent(wrap) && verifyEvent(seal) && Boolean(unwrapped) && unwrapped.kind === 14 && unwrapped.pubkey === pubs.ALICE && unwrapped.content === plaintext && unwrapped.tags.some((tag) => tag[0] === 'p' && tag[1] === pubs.BOB) && outsiderCannotDecrypt, { published, outer_signature_valid: verifyEvent(wrap), seal_signature_valid: verifyEvent(seal), sender_pubkey: unwrapped?.pubkey, recipient_pubkey: pubs.BOB, decryption_matches: unwrapped?.content === plaintext, outsider_pubkey: pubkey(keys.OUTSIDER), outsider_decryption_rejected: outsiderCannotDecrypt, recipient_retrieved: Boolean(received) });
	});
	await check('nip17_recipient_only_visibility', async () => {
		const selfRead = await alice.read({ kinds: [1059], '#p': [pubs.ALICE], limit: 50 });
		const wrongRecipientFilter = await alice.read({ kinds: [1059], '#p': [pubs.BOB], limit: 50 });
		const delivered = results.find((entry) => entry.name === 'nip17_recent_timestamp_delivery_and_decryption')?.published?.event_id;
		return result(!selfRead.events.some((item) => item.id === delivered) && wrongRecipientFilter.outcome === 'CLOSED' && wrongRecipientFilter.events.length === 0, { sender_filter_events: selfRead.events.length, wrong_recipient_filter_outcome: wrongRecipientFilter.outcome, wrong_recipient_event_count: wrongRecipientFilter.events.length });
	});
	if (process.env.POC_HOLD === '1') await desktopHold();
	await check('open_pre_revocation_subscriptions', async () => {
		bobSubscriptions = await Promise.all([
			bob.subscribe({ kinds: [9], '#h': [publicChannel], since: Math.floor(Date.now() / 1000) }),
			bob.subscribe({ kinds: [7], '#h': [publicChannel], since: Math.floor(Date.now() / 1000) }),
			bob.subscribe({ kinds: [9], '#h': [privateChannel], since: Math.floor(Date.now() / 1000) }),
		]);
		return result(bobSubscriptions.length === 3 && bobSubscriptions.every((subscription) => subscription.outcome === 'EOSE'), { active_subscriptions: bobSubscriptions.length, outcomes: bobSubscriptions.map((subscription) => subscription.outcome), private_history_previously_read: true });
	});
	await check('nip43_remove_bob', async () => {
		bobRemove = await event(keys.OWNER, 9031, [['p', pubs.BOB]]);
		const wire = await owner.publish(bobRemove);
		return result(wire.accepted, wire);
	});
	await check('nip43_remove_exact_replay', async () => {
		const wire = await owner.publish(bobRemove);
		return result(wire.outcome === 'OK' && !wire.accepted, wire);
	});
	await check('revoked_existing_subscription', async () => {
		const marker = await event(keys.ALICE, 9, [['h', publicChannel]], `POC ${randomUUID()}`);
		const sent = await alice.publish(marker);
		const socketFrame = await bob.next((f) => ['SOCKET_CLOSE', 'TRANSPORT_ERROR'].includes(f[0]), 2500);
		const leaked = bob.observations.some((frame) => frame.type === 'EVENT' && frame.event_id === marker.id);
		const closedSubscriptions = bob.observations.filter((frame) => frame.type === 'CLOSED' && bobSubscriptions.some((subscription) => subscription.sub === frame.subscription)).map((frame) => frame.subscription);
		return result(sent.accepted && socketFrame[0] === 'SOCKET_CLOSE' && !leaked, { published: sent, opened_subscription_count: bobSubscriptions.length, closed_subscription_count: closedSubscriptions.length, socket_outcome: socketFrame[0], post_removal_marker_visible: leaked, observation_ms: 2500, socket_open: bob.socket.readyState === WebSocket.OPEN });
	});
	await check('revoked_existing_publish', async () => {
		if (bob.socket.readyState === WebSocket.CLOSED) return { name: 'revoked_existing_publish', pass: null, outcome: 'UNVERIFIED_SOCKET_CLOSED_BEFORE_PUBLISH', attempted: false };
		const wire = await bob.publish(await event(keys.BOB, 9, [['h', publicChannel]], `POC ${randomUUID()}`));
		return result(!wire.accepted, { ...wire, attempted: true });
	});
	await check('revoked_existing_new_req', async () => {
		if (bob.socket.readyState === WebSocket.CLOSED) return { name: 'revoked_existing_new_req', pass: null, outcome: 'UNVERIFIED_SOCKET_CLOSED_BEFORE_REQ', attempted: false };
		const read = await bob.read({ kinds: [9], '#h': [publicChannel], limit: 20 });
		return result(deniedWithoutEvents(read), { ...read, attempted: true });
	});
	await check('revoked_fresh_connection', async () => {
		const joined = await connect(url, keys.BOB);
		const privateRead = joined.auth.accepted ? await joined.session.read({ kinds: [9], '#h': [privateChannel], limit: 20 }) : null;
		return result(explicitAuthDenial(joined.auth), { auth: joined.auth, private_read: privateRead ? { outcome: privateRead.outcome, event_count: privateRead.events.length } : null });
	});
	await check('revoked_reconnect_after_delay', async () => {
		await new Promise((resolve) => setTimeout(resolve, 1500));
		const joined = await connect(url, keys.BOB);
		return result(explicitAuthDenial(joined.auth), { delay_ms: 1500, auth: joined.auth, private_content_read: false });
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
			const relayRejected = (joined.auth.websocket_open && explicitAuthDenial(joined.auth)) || [400, 404, 421].includes(joined.auth.http_status);
			return result(relayRejected, { ...joined.auth, requested_host: new URL(process.env.POC_WRONG_HOST_URL).host, dns_target: '127.0.0.1', layer: joined.auth.websocket_open ? 'relay_websocket_auth' : joined.auth.http_status ? 'relay_http_upgrade' : 'unclassified_transport' });
		});
	} else results.push({ name: 'wrong_host_denied', pass: null, outcome: 'UNVERIFIED_NO_HOST_ROUTE' });
	if (process.env.POC_TLS_FAILURE_URL) {
		await check('wrong_sni_tls_rejected_before_relay', async () => {
			const joined = await connect(process.env.POC_TLS_FAILURE_URL, keys.ALICE);
			return result(!joined.auth.websocket_open && !joined.auth.challenged && joined.auth.outcome === 'TRANSPORT_ERROR' && joined.auth.transport_error_code === 'ERR_TLS_CERT_ALTNAME_INVALID', { outcome: joined.auth.outcome, transport_error_code: joined.auth.transport_error_code, websocket_open: joined.auth.websocket_open, challenged: joined.auth.challenged, requested_sni: new URL(process.env.POC_TLS_FAILURE_URL).hostname, dns_target: '127.0.0.1', expected_layer: 'tls_certificate_validation' });
		});
	} else results.push({ name: 'wrong_sni_tls_rejected_before_relay', pass: null, outcome: 'UNVERIFIED_NO_TLS_NEGATIVE_TEST' });
} finally {
	for (const session of sessions) session.close();
	console.log(JSON.stringify({ relay: url, public_keys: pubs, public_channel: publicChannel, private_channel: privateChannel, public_message_id: publicMessage?.id, private_message_id: privateMessage?.id, results }, null, 2));
}
if (results.some((item) => item.pass === false)) process.exitCode = 1;
