import { createHash, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:https';

const relayPubkey = process.env.BUZZ_TEST_RACE_RELAY_PUBKEY;
const controlToken = process.env.BUZZ_TEST_RACE_CONTROL_TOKEN;
if (!/^[0-9a-f]{64}$/.test(relayPubkey || '')) {
	throw new Error('BUZZ_TEST_RACE_RELAY_PUBKEY must be 64 lowercase hex characters');
}
if (!controlToken || controlToken.length < 32) {
	throw new Error('BUZZ_TEST_RACE_CONTROL_TOKEN must contain at least 32 characters');
}

function authorized(request) {
	const supplied = request.headers.authorization || '';
	const expected = `Bearer ${controlToken}`;
	const suppliedBytes = Buffer.from(supplied);
	const expectedBytes = Buffer.from(expected);
	return suppliedBytes.length === expectedBytes.length && timingSafeEqual(suppliedBytes, expectedBytes);
}

let waiting = false;
let releaseUpgrade;
const server = createServer({
	cert: readFileSync(process.env.TLS_CERT_FILE),
	key: readFileSync(process.env.TLS_KEY_FILE),
}, (request, response) => {
	if (request.method === 'GET' && request.url === '/') {
		const body = JSON.stringify({ self: relayPubkey });
		response.writeHead(200, {
			'Content-Type': 'application/nostr+json',
			'Content-Length': Buffer.byteLength(body),
			'Connection': 'keep-alive',
		});
		response.end(body);
		return;
	}
	if (request.method === 'GET' && request.url === '/__test/status' && authorized(request)) {
		response.writeHead(200, { 'Content-Type': 'application/json' });
		response.end(JSON.stringify({ waiting }));
		return;
	}
	if (request.method === 'POST' && request.url === '/__test/release' && authorized(request) && waiting) {
		response.writeHead(204);
		response.end();
		releaseUpgrade?.();
		return;
	}
	response.writeHead(404);
	response.end();
});

server.on('upgrade', async (request, socket) => {
	const key = request.headers['sec-websocket-key'];
	const connectionTokens = (request.headers.connection || '')
		.split(',')
		.map((token) => token.trim().toLowerCase());
	const decodedKey = typeof key === 'string' ? Buffer.from(key, 'base64') : Buffer.alloc(0);
	if (
		request.method !== 'GET' ||
		request.url !== '/' ||
		request.headers.upgrade?.toLowerCase() !== 'websocket' ||
		!connectionTokens.includes('upgrade') ||
		request.headers['sec-websocket-version'] !== '13' ||
		typeof key !== 'string' ||
		decodedKey.length !== 16 ||
		decodedKey.toString('base64') !== key
	) {
		socket.destroy();
		return;
	}
	waiting = true;
	console.log(JSON.stringify({ stage: 'upgrade-held' }));
	await new Promise((resolve) => {
		releaseUpgrade = resolve;
		const timeout = setTimeout(resolve, 60000);
		timeout.unref();
	});
	waiting = false;
	releaseUpgrade = undefined;
	const accept = createHash('sha1')
		.update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
		.digest('base64');
	socket.write(
		`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`,
	);
	setTimeout(() => socket.end(), 5000).unref();
});

server.listen(Number(process.env.PORT || 443), '0.0.0.0', () => {
	console.log(JSON.stringify({ stage: 'ready' }));
});
