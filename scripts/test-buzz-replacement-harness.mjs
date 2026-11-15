import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { readFileSync } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const source = readFileSync(
	new URL('../frontend/scripts/buzz-replacement-e2e.mjs', import.meta.url),
	'utf8'
);

test('SIGKILL satisfies the actual crash-recovery wait predicate', async () => {
	const predicate = source
		.match(/'post-ACK worker crash',\s*async \(\) => ([^\n;]+)/)[1]
		.replace(/\)$/, '');
	const backend = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)']);
	try {
		const exit = once(backend, 'exit');
		backend.kill('SIGKILL');
		await exit;
		assert.equal(backend.exitCode, null);
		assert.equal(runInNewContext(predicate, { backend }), true);
	} finally {
		backend.kill('SIGKILL');
	}
});

test('worker restart discards held commands from closed sockets', () => {
	const reset = source.match(/fault\.holdCommand = null;([\s\S]*?)fault\.afterAck =/)[1];
	const fault = { heldCommands: [{ command: { id: 'earlier-operation' } }] };
	runInNewContext(reset, { fault });
	assert.equal(fault.heldCommands.length, 0);
});

test('ACK crash injection is armed before the worker restarts', () => {
	const restart = source.slice(
		source.indexOf('\tfault.holdCommand = null;\n', source.indexOf("'pre-ACK revoke held'")),
		source.indexOf("\tawait until(\n\t\t'post-ACK worker crash'")
	);
	const fault = { heldCommands: [] };
	let starts = 0;
	runInNewContext(restart, {
		fault,
		backendBinary: 'unused',
		backendEnv: {},
		start() {
			assert.equal(typeof fault.afterAck, 'function');
			starts++;
		}
	});
	assert.equal(starts, 1);
});

test('membership assertion can observe relay state while worker reads remain held', async () => {
	const fault = { read: 'hold', observations: [], held: [] };
	const servers = [];
	const upstream = createServer((_req, res) => res.writeHead(200).end('{"send_enabled":false}'));
	await new Promise((resolve) => upstream.listen(0, '127.0.0.1', resolve));
	try {
		// Execute the real proxy; this HTTP regression does not open a WebSocket.
		const proxy = runInNewContext(
			source.slice(source.indexOf('async function proxy('), source.indexOf('const backendBinary')) +
				'\nproxy',
			{
				createServer,
				httpRequest,
				Buffer,
				fault,
				servers,
				WebSocketServer: class {
					on() {}
				}
			}
		);
		const port = await proxy(upstream.address().port);
		const url = `http://127.0.0.1:${port}/api/v1/relay/membership/check`;
		const heldRead = fetch(url);
		const heldDeadline = Date.now() + 2000;
		while (!fault.held.length && Date.now() < heldDeadline)
			await new Promise((resolve) => setTimeout(resolve, 5));
		assert.equal(fault.held.length, 1, 'proxy did not hold the worker read before the deadline');
		const observation = await fetch(url, {
			headers: { 'x-elembra-test-observation': '1' },
			signal: AbortSignal.timeout(1000)
		});
		assert.deepEqual(await observation.json(), { send_enabled: false });
		assert.equal(fault.held.length, 1, 'worker read must stay held');
		fault.held.splice(0).forEach((release) => release());
		assert.equal((await heldRead).status, 200);
	} finally {
		fault.held.splice(0).forEach((release) => release());
		for (const { server } of servers) {
			server.closeAllConnections();
			await new Promise((resolve) => server.close(resolve));
		}
		upstream.closeAllConnections();
		await new Promise((resolve) => upstream.close(resolve));
	}
});

test('debug qualification and its regression fixture match the runner source pin', () => {
	const runner = readFileSync(new URL('./run-buzz-replacement-e2e.sh', import.meta.url), 'utf8');
	const rust = readFileSync(
		new URL('../backend/server/src/handlers/chat_identity.rs', import.meta.url),
		'utf8'
	);
	const sha = runner.match(/checkout --detach ([0-9a-f]{40})/)[1];
	const workflow = readFileSync(
		new URL('../.github/workflows/integration-tests.yml', import.meta.url),
		'utf8'
	);
	const checkoutPins = [...workflow.matchAll(/repository: kubedoio\/buzz\s+ref: ([0-9a-f]{40})/g)];
	assert.equal(checkoutPins.length, 2);
	for (const [, pin] of checkoutPins) assert.equal(pin, sha);
	assert.equal(rust.match(/const SOURCE_SHA: &str = "([0-9a-f]+)"/)[1], sha);
	assert.equal(rust.match(/let source = "([0-9a-f]+)"/)[1], sha);
	assert.equal(source.match(/assert.equal\(manifest.buzz_source, '([0-9a-f]+)'\)/)[1], sha);
	const tree = source.match(/assert.equal\(manifest.buzz_tree, '([0-9a-f]+)'\)/)[1];
	assert.equal(rust.match(/const SOURCE_TREE: &str = "([0-9a-f]+)"/)[1], tree);
	assert.equal(rust.match(/let source_tree = "([0-9a-f]+)"/)[1], tree);
});

test('completion fault requires its exact attempt marker as well as a retry audit', async () => {
	const scenario = source.slice(
		source.indexOf('\tconst dbFailure ='),
		source.indexOf('\tconst recovery =')
	);
	let auditObserved = false;
	let triggerInstalled = false;
	let injectionAttempted = false;
	let sequenceDropped = false;
	let createSql = '';
	await runInNewContext(`(async () => {${scenario}})()`, {
		assert,
		db: { container: 'fixture' },
		onboard: async () => ({ candidate: { pk: 'candidate' } }),
		replacement: async () => ({ operation_id: 'operation' }),
		quote: (value) => `'${value}'`,
		sql(_container, query) {
			if (query.includes('CREATE FUNCTION')) {
				triggerInstalled = true;
				createSql = query;
			}
			if (query === 'SELECT is_called FROM replacement_e2e_completion_attempt')
				return injectionAttempted ? 't' : 'f';
			if (query.startsWith('DROP TRIGGER')) {
				triggerInstalled = false;
				sequenceDropped = query.includes('DROP SEQUENCE replacement_e2e_completion_attempt');
			}
		},
		rows() {
			if (injectionAttempted && triggerInstalled) auditObserved = true;
			return [{ audit_id: injectionAttempted ? 'injected-failure-retry' : 'unrelated-retry' }];
		},
		until: async (name, check) => {
			if (name === 'completion transaction failure audited') {
				assert.equal(await check(), false, 'staged membership and unrelated retry cannot qualify');
				injectionAttempted = true;
			}
			assert.equal(await check(), true);
		},
		initiate: async () => {},
		membership: async () => ({ membership: 'active', send_enabled: false }),
		operationStatus: async () => ({ state: 'pending' }),
		assertGated: async () => {},
		completed: async () => {},
		assertFreshEvidence() {},
		pass() {},
		relayBase: 'unused',
		community: 'unused'
	});
	assert.equal(injectionAttempted, true);
	assert.equal(auditObserved, true);
	assert.equal(triggerInstalled, false);
	assert.equal(sequenceDropped, true);
	assert.match(createSql, /NEW.operation_id='operation'::uuid/);
	assert.match(createSql, /PERFORM nextval\('replacement_e2e_completion_attempt'\)/);
});

test('mapping rotation is durably rejected before its original pin is restored', async () => {
	const scenario = source.slice(
		source.indexOf('\tconst rotated ='),
		source.indexOf('\tconst dbFailure =')
	);
	let workerObserved = false;
	let patches = 0;
	await runInNewContext(`(async () => {${scenario}})()`, {
		assert,
		onboard: async () => ({ candidate: { pk: 'candidate' } }),
		replacement: async () => ({ operation_id: 'operation' }),
		key: () => ({ pk: 'rotated' }),
		fault: { heldCommands: [{ release() {} }] },
		initiate: async () => {},
		until: async (name, check) => {
			if (name === 'mapping rotation durably blocks the operation') workerObserved = true;
			assert.equal(await check(), true);
		},
		delay: async () => {},
		api: async () => {
			if (++patches === 2)
				assert.equal(
					workerObserved,
					true,
					'original pin restored before worker rejected the change'
				);
		},
		operationStatus: async () => ({ state: workerObserved ? 'blocked' : 'pending' }),
		assertGated: async () => {},
		pass() {},
		base: 'unused',
		admin: {},
		tenant: 'tenant',
		relayUrl: 'unused',
		relay: { pk: 'original' }
	});
	assert.equal(workerObserved, true);
	assert.equal(patches, 2);
});
