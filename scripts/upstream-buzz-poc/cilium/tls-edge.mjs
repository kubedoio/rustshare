// Disposable TLS edge for the Cilium qualification fixture.
// It forwards only the configured service and never accepts client proxy URLs.
import { readFileSync } from 'node:fs';
import { connect } from 'node:net';
import { createServer } from 'node:tls';

const required = (name) => {
	const value = process.env[name];
	if (!value) throw new Error(`${name} is required`);
	return value;
};

const upstreamHost = required('UPSTREAM_HOST');
const upstreamPort = Number(required('UPSTREAM_PORT'));
const listenPort = Number(required('LISTEN_PORT'));
const expectedHost = required('EXPECTED_HOST');
const headerLimit = 64 * 1024;
if (![upstreamPort, listenPort].every((port) => Number.isInteger(port) && port > 0 && port <= 65535)) {
	throw new Error('ports must be valid TCP ports');
}
if (!/^[a-z0-9.-]+$/.test(upstreamHost) || !/^[a-z0-9.-]+$/.test(expectedHost)) {
	throw new Error('hosts must be canonical DNS names');
}

const server = createServer({
	cert: readFileSync(required('TLS_CERT_FILE')),
	key: readFileSync(required('TLS_KEY_FILE')),
}, (client) => {
	let pending = Buffer.alloc(0);
	let connecting = false;
	let forwarding = false;
	let upstream;
	const reject = () => client.destroy();
	const timeout = setTimeout(reject, 10_000);
	client.on('error', () => upstream?.destroy());
	client.on('close', () => {
		clearTimeout(timeout);
		upstream?.destroy();
	});
	client.on('data', (chunk) => {
		if (forwarding) return;
		pending = Buffer.concat([pending, chunk]);
		if (pending.length > headerLimit) return reject();
		if (connecting) return;
		const end = pending.indexOf('\r\n\r\n');
		if (end < 0) return;
		const header = pending.subarray(0, end).toString('latin1');
		const host = header.match(/^Host:\s*([^\r\n]+)$/im)?.[1]?.trim().toLowerCase();
		if (host !== expectedHost || !/^GET |^POST |^HEAD /m.test(header)) return reject();
		connecting = true;
		clearTimeout(timeout);
		upstream = connect({ host: upstreamHost, port: upstreamPort });
		upstream.setTimeout(10_000, reject);
		upstream.once('connect', () => {
			upstream.write(pending);
			pending = Buffer.alloc(0);
			forwarding = true;
			client.pipe(upstream);
			upstream.pipe(client);
		});
		upstream.on('error', reject);
	});
});

server.on('error', (error) => {
	process.stderr.write(`tls_edge_start_failed code=${error.code || 'ERROR'}\n`);
	process.exitCode = 1;
});
server.listen(listenPort, '0.0.0.0');
