// Disposable TLS termination for the stock Buzz desktop client's required WSS URL.
import { readFileSync } from 'node:fs';
import { createServer } from 'node:tls';
import { connect } from 'node:net';

const [certPath, keyPath, listenPort, targetPort] = process.argv.slice(2);
createServer({ cert: readFileSync(certPath), key: readFileSync(keyPath) }, (client) => {
  const relay = connect(Number(targetPort), '127.0.0.1');
  client.pipe(relay).pipe(client);
  client.on('error', () => relay.destroy());
  relay.on('error', () => client.destroy());
}).listen(Number(listenPort), '::');
