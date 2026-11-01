// Disposable TLS termination for the stock Buzz desktop client's required WSS URL.
import { readFileSync } from 'node:fs';
import { createServer } from 'node:tls';
import { connect } from 'node:net';

const [certPath, keyPath, listenPort, targetPort] = process.argv.slice(2);
let connectionId = 0;
const forward = (client) => {
	const id = ++connectionId;
	const sni = client.servername || '';
	const chunks = [];
	let bytes = 0;
	const onHeader = (chunk) => {
		chunks.push(chunk);
		bytes += chunk.length;
		const requestBytes = Buffer.concat(chunks);
		if (bytes > 65536) {
			client.destroy();
			console.log(JSON.stringify({ id, stage: 'request_header_too_large', sni }));
			return;
		}
		if (!requestBytes.includes('\r\n\r\n')) return;
		client.pause();
		client.removeListener('data', onHeader);
		const header = requestBytes.toString('latin1').split('\r\n\r\n', 1)[0];
		const host = header.match(/^Host:\s*(.+)$/im)?.[1]?.trim() || '';
		const request = header.split('\r\n', 1)[0] || '';
		const relay = connect(Number(targetPort), '127.0.0.1');
		relay.once('connect', () => {
			console.log(JSON.stringify({ id, stage: 'proxy_forwarded', sni, host, request }));
			relay.write(requestBytes);
			client.pipe(relay);
			relay.pipe(client);
			client.resume();
		});
		relay.once('error', (error) => {
			console.log(JSON.stringify({ id, stage: 'relay_connect_failed', sni, host, code: error.code || 'ERROR' }));
			client.destroy();
		});
	};
	client.on('data', onHeader);
};
const tlsOptions = { cert: readFileSync(certPath), key: readFileSync(keyPath) };
const v4 = createServer(tlsOptions, forward);
const v6 = createServer(tlsOptions, forward);
for (const server of [v4, v6]) {
	server.on('tlsClientError', (error, client) => {
		console.log(JSON.stringify({ stage: 'tls_rejected', sni: client.servername || '', code: error.code || 'ERROR' }));
	});
	server.on('error', (error) => {
		console.log(JSON.stringify({ stage: 'proxy_listen_failed', code: error.code || 'ERROR' }));
		process.exitCode = 1;
	});
}
v4.listen(Number(listenPort), '127.0.0.1');
v6.listen(Number(listenPort), '::1');
