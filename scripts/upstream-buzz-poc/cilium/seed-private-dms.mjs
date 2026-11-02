// Seed two stock Buzz communities and their native NIP-29 private DMs.
// Stdout contains public fixture metadata only; credentials are env inputs.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { request as httpsRequest } from 'node:https';
import { randomUUID } from 'node:crypto';
import WebSocket from 'ws';
import { finalizeEvent, getPublicKey } from 'nostr-tools';

const required = (name) => {
	const value = process.env[name];
	if (!value) throw new Error(`${name}_missing`);
	return value;
};
const secret = (name) => Buffer.from(required(name), 'hex');
const timeout = (ms, label) => new Promise((_, reject) => {
	const timer = setTimeout(() => reject(new Error(`${label}_timeout`)), ms);
	timer.unref();
});

class RelaySession {
	constructor(url, ca) {
		this.url = url;
		this.socket = new WebSocket(url, { ca, perMessageDeflate: false });
		this.frames = [];
		this.waiters = [];
		this.socket.on('message', (data) => {
			let frame;
			try { frame = JSON.parse(String(data)); } catch { return; }
			const index = this.waiters.findIndex((waiter) => waiter.predicate(frame));
			if (index >= 0) this.waiters.splice(index, 1)[0].resolve(frame);
			else this.frames.push(frame);
		});
	}
	async open() {
		await Promise.race([
			new Promise((resolve, reject) => {
				this.socket.once('open', resolve);
				this.socket.once('error', reject);
			}),
			timeout(10_000, 'websocket_open'),
		]);
	}
	async wait(predicate, label) {
		const index = this.frames.findIndex(predicate);
		if (index >= 0) return this.frames.splice(index, 1)[0];
		return Promise.race([
			new Promise((resolve) => this.waiters.push({ predicate, resolve })),
			timeout(10_000, label),
		]);
	}
	async auth(privateKey) {
		const challenge = await this.wait((frame) => frame[0] === 'AUTH', 'nip42_challenge');
		const proof = finalizeEvent({
			kind: 22242,
			created_at: Math.floor(Date.now() / 1000),
			tags: [['relay', this.url], ['challenge', challenge[1]]],
			content: '',
		}, privateKey);
		this.socket.send(JSON.stringify(['AUTH', proof]));
		const answer = await this.wait((frame) => frame[0] === 'OK' && frame[1] === proof.id, 'nip42_answer');
		if (answer[2] !== true) throw new Error('nip42_auth_denied');
	}
	async publish(event) {
		this.socket.send(JSON.stringify(['EVENT', event]));
		const answer = await this.wait((frame) => frame[0] === 'OK' && frame[1] === event.id, `kind_${event.kind}_publish`);
		if (answer[2] !== true) throw new Error(`kind_${event.kind}_publish_denied`);
	}
	async discovery(channelId) {
		const subId = randomUUID();
		this.socket.send(JSON.stringify(['REQ', subId, { kinds: [39000], '#d': [channelId] }]));
		const events = [];
		for (;;) {
			const frame = await this.wait((item) =>
				(item[0] === 'EOSE' && item[1] === subId) || (item[0] === 'EVENT' && item[1] === subId),
				'kind_39000_discovery',
			);
			if (frame[0] === 'EOSE') return events;
			events.push(frame[2]);
		}
	}
	close() { this.socket.close(); }
}

const signed = (privateKey, kind, tags, content = '') => finalizeEvent({
	kind,
	created_at: Math.floor(Date.now() / 1000),
	tags,
	content,
}, privateKey);

function jsonRequest(url, method, body, headers, ca) {
	return new Promise((resolve, reject) => {
		const request = httpsRequest(url, { method, headers, ca }, (response) => {
			const chunks = [];
			response.on('data', (chunk) => chunks.push(chunk));
			response.on('end', () => {
				const bytes = Buffer.concat(chunks);
				let value;
				try { value = JSON.parse(bytes.toString('utf8')); } catch { return reject(new Error('json_response_invalid')); }
				if (response.statusCode < 200 || response.statusCode >= 300) {
					return reject(new Error(`operator_http_${response.statusCode}`));
				}
				resolve(value);
			});
		});
		request.setTimeout(10_000, () => request.destroy(new Error('https_timeout')));
		request.on('error', reject);
		request.end(body);
	});
}

async function community(url, ownerKey, operatorKey, ca) {
	const operatorUrl = new URL('/operator/communities', url.replace(/^wss:/, 'https:'));
	const requestUrl = operatorUrl.toString();
	const ownerPubkey = getPublicKey(ownerKey);
	const body = Buffer.from(JSON.stringify({
		host: operatorUrl.host,
		initial_owner_pubkey: ownerPubkey,
	}));
	const signature = finalizeEvent({
		kind: 27235,
		created_at: Math.floor(Date.now() / 1000),
		tags: [
			['u', requestUrl],
			['method', 'POST'],
			['payload', createHash('sha256').update(body).digest('hex')],
		],
		content: '',
	}, operatorKey);
	const authorization = `Nostr ${Buffer.from(JSON.stringify(signature)).toString('base64')}`;
	const response = await jsonRequest(requestUrl, 'POST', body, {
		'content-type': 'application/json',
		'content-length': String(body.length),
		authorization,
	}, ca);
	if (!/^[0-9a-f-]{36}$/.test(response.community_id || '')) throw new Error('community_id_missing');
	return response.community_id;
}

async function relayIdentity(url, ca) {
	const requestUrl = new URL('/', url.replace(/^wss:/, 'https:'));
	const response = await jsonRequest(requestUrl, 'GET', undefined, {
		accept: 'application/nostr+json',
	}, ca);
	if (!/^[0-9a-f]{64}$/.test(response.self || '')) throw new Error('nip11_self_missing');
	return response.self;
}

async function seed(label) {
	const suffix = label.toUpperCase();
	const relayUrl = required(`RELAY_${suffix}_URL`);
	const relayCa = readFileSync(required(`ROOT_${suffix}_PEM`));
	const ownerKey = secret(`OWNER_${suffix}_SK`);
	const memberKey = secret(`MEMBER_${suffix}_SK`);
	const friendKey = secret(`FRIEND_${suffix}_SK`);
	const operatorKey = secret(`OPERATOR_${suffix}_SK`);
	const ownerPubkey = getPublicKey(ownerKey);
	const memberPubkey = getPublicKey(memberKey);
	const friendPubkey = getPublicKey(friendKey);
	const relayPubkey = await relayIdentity(relayUrl, relayCa);
	const communityId = await community(relayUrl, ownerKey, operatorKey, relayCa);

	const admin = new RelaySession(relayUrl, relayCa);
	await admin.open();
	await admin.auth(ownerKey);
	await admin.publish(signed(ownerKey, 9030, [['p', memberPubkey], ['role', 'member']]));
	await admin.publish(signed(ownerKey, 9030, [['p', friendPubkey], ['role', 'member']]));
	admin.close();

	const user = new RelaySession(relayUrl, relayCa);
	await user.open();
	await user.auth(memberKey);
	const channelId = randomUUID();
	await user.publish(signed(memberKey, 9007, [
		['h', channelId],
		['name', `Disposable native private DM ${label}`],
		['channel_type', 'dm'],
		['visibility', 'private'],
	]));
	await user.publish(signed(memberKey, 9000, [['h', channelId], ['p', friendPubkey]]));
	const message = signed(memberKey, 9, [['h', channelId]], `disposable-private-dm-${randomUUID()}`);
	await user.publish(message);
	const discovery = await user.discovery(channelId);
	const metadata = discovery.find((event) => event.tags?.some((tag) => tag[0] === 'd' && tag[1] === channelId));
	const tagged = (name) => metadata?.tags?.some((tag) => tag[0] === name) === true;
	if (!tagged('private') || !tagged('hidden')) throw new Error('native_private_dm_metadata_failed');
	user.close();
	return {
		community_id: communityId,
		relay_pubkey: relayPubkey,
		channel_id: channelId,
		event_id: message.id,
		author: memberPubkey,
		friend_pubkey: friendPubkey,
		private: true,
		hidden: true,
		discovery_count: discovery.length,
	};
}

try {
	const [a, b] = await Promise.all([seed('a'), seed('b')]);
	process.stdout.write(`${JSON.stringify({ ok: true, tenants: { a, b } })}\n`);
} catch {
	process.stderr.write('private_dm_seed_failed\n');
	process.exitCode = 1;
}
