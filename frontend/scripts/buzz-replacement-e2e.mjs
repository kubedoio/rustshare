// Real process acceptance. SQL is limited to fixture setup, assertions and named faults.
import assert from 'node:assert/strict';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, openSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { createServer as tcpServer } from 'node:net';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { schnorr } from '@noble/curves/secp256k1.js';
import { WebSocketServer, WebSocket as ProxySocket } from 'ws';

const root = fileURLToPath(new URL('../../', import.meta.url));
const artifacts = resolve(process.env.REPLACEMENT_E2E_ARTIFACTS);
mkdirSync(artifacts, { recursive: true, mode: 0o700 });
const prefix = `replacement-e2e-${randomUUID().slice(0, 8)}`;
const containers = [], children = [], servers = [], results = [], secrets = [];
let interrupted = false;
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => {
  interrupted = true;
  for (const child of children) child.kill('SIGTERM');
});
const secret = () => { const value = randomBytes(32).toString('hex'); secrets.push(value); return value; };
const key = () => { const sk = secret(); return { sk, pk: Buffer.from(schnorr.getPublicKey(Buffer.from(sk, 'hex'))).toString('hex') }; };
const service = key(), relay = key(), password = secret();
const hash = (value) => createHash('sha256').update(value).digest('hex');
const redact = (value) => secrets.reduce((text, s) => text.replaceAll(s, '<redacted>'), String(value));
const docker = (...args) => execFileSync('docker', args, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
const quote = (value) => `'${String(value).replaceAll("'", "''")}'`;
const sql = (container, query) => docker('exec', '-i', container, 'psql', '-U', 'rustshare', '-d', 'rustshare_test', '-v', 'ON_ERROR_STOP=1', '-At', '-c', query);
const rows = (container, query) => JSON.parse(sql(container, `SELECT coalesce(json_agg(t),'[]'::json) FROM (${query}) t`));
const pass = (name) => { results.push({ name, status: 'passed' }); console.log(`PASS ${name}`); };
async function until(name, check, timeout = 90_000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (interrupted) throw new Error('acceptance interrupted');
    if (await check()) return;
    await delay(250);
  }
  throw new Error(`timeout: ${name}`);
}
async function freePort() {
  const server = tcpServer();
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const port = server.address().port;
  await new Promise((r) => server.close(r));
  return port;
}
function start(binary, env, name) {
  const fd = openSync(`${artifacts}/${name}.log`, 'a', 0o600);
  // Explicit environment prevents inherited deployment endpoints, secrets and proxies.
  const child = spawn(binary, [], { cwd: artifacts, env: { PATH: process.env.PATH, HOME: process.env.HOME, ...env }, stdio: ['ignore', fd, fd] });
  children.push(child);
  return child;
}
async function stop(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGTERM');
  await Promise.race([new Promise((r) => child.once('exit', r)), delay(10_000)]);
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGKILL');
    await new Promise((r) => child.once('exit', r));
  }
}
async function dependency(name, image, ports, env) {
  const container = `${prefix}-${name}`;
  containers.push(container);
  docker('run', '-d', '--rm', '--name', container, '--label', `io.elembra.replacement-e2e=${process.env.GITHUB_RUN_ID || prefix}`, ...ports.flatMap((p) => ['-p', `127.0.0.1::${p}`]), ...Object.entries(env).flatMap(([k, v]) => ['-e', `${k}=${v}`]), image);
  return { container, port: (p) => Number(docker('port', container, `${p}/tcp`).split(':').at(-1)) };
}
function event(kind, tags, content, signer) {
  const e = { pubkey: signer.pk, created_at: Math.floor(Date.now() / 1000), kind, tags, content };
  e.id = hash(JSON.stringify([0, e.pubkey, e.created_at, kind, tags, content]));
  e.sig = Buffer.from(schnorr.sign(Buffer.from(e.id, 'hex'), Buffer.from(signer.sk, 'hex'))).toString('hex');
  return e;
}
function verify(e, pin) {
  assert.equal(e.pubkey, pin);
  assert.equal(e.id, hash(JSON.stringify([0, e.pubkey, e.created_at, e.kind, e.tags, e.content])));
  assert(schnorr.verify(Buffer.from(e.sig, 'hex'), Buffer.from(e.id, 'hex'), Buffer.from(pin, 'hex')));
  assert(Math.abs(Date.now() / 1000 - e.created_at) < 30);
  return JSON.parse(e.content);
}
async function relayHttp(base, method, path, body) {
  const payload = body === undefined ? undefined : JSON.stringify(body);
  const tags = [['u', `${base}${path}`], ['method', method], ['nonce', randomUUID()]];
  if (payload !== undefined) tags.push(['payload', hash(payload)]);
  const auth = Buffer.from(JSON.stringify(event(27235, tags, '', service))).toString('base64');
  const response = await fetch(`${base}${path}`, { method, body: payload, headers: { authorization: `Nostr ${auth}`, 'content-type': 'application/json' }, signal: AbortSignal.timeout(10_000) });
  assert.equal(response.status, 200, `relay ${path}: ${await response.clone().text()}`);
  return response.json();
}
async function membership(base, community, pubkey) {
  const request = { request_id: randomUUID(), community_id: community, pubkey };
  const signed = await relayHttp(base, 'POST', '/api/v1/relay/membership/check', request);
  const result = verify(signed, relay.pk);
  for (const [k, v] of Object.entries(request)) assert.equal(result[k], v);
  assert.equal(signed.kind, 19030);
  assert(Math.abs(Date.now() / 1000 - result.evaluated_at) < 30);
  return result;
}
async function wire(url, signer, command) {
  return new Promise((resolveResult, reject) => {
    const socket = new WebSocket(url);
    const timer = setTimeout(() => { socket.close(); reject(new Error('wire timeout')); }, 5000);
    const finish = (result) => { clearTimeout(timer); socket.close(); resolveResult(result); };
    socket.onmessage = ({ data }) => {
      const m = JSON.parse(String(data));
      if (m[0] === 'AUTH') socket.send(JSON.stringify(['AUTH', event(22242, [['relay', url], ['challenge', m[1]]], '', signer)]));
      if (m[0] === 'OK' && m[1] !== command.id) {
        if (m[2] === true) socket.send(JSON.stringify(['EVENT', command])); else finish(m);
        return;
      }
      if (m[0] === 'OK' && (m[2] || !String(m[3]).toLowerCase().includes('auth'))) finish(m);
    };
    socket.onerror = () => { clearTimeout(timer); reject(new Error('wire connection error')); };
  });
}
async function api(base, session, method, path, body, expected = 200) {
  const headers = { 'content-type': 'application/json' };
  if (session) headers.authorization = `Bearer ${session.token}`;
  const response = await fetch(`${base}/api/v1${path}`, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(30_000) });
  const text = await response.text();
  assert([expected].flat().includes(response.status), `${method} ${path}: expected ${expected}, got ${response.status}: ${redact(text)}`);
  return text ? JSON.parse(text) : null;
}
// The proxy never grants membership: it can withhold ACKs/reads or corrupt evidence.
const fault = { read: null, dropAckFor: null, afterAck: null, dropped: [], commands: [], observations: [], held: [] };
async function proxy(targetPort) {
  const server = createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      if (req.url.endsWith('/membership/check') && fault.read === 'unavailable') { res.writeHead(503).end(); return; }
      const upstream = httpRequest({ host: '127.0.0.1', port: targetPort, path: req.url, method: req.method, headers: req.headers }, (response) => {
        const bytes = [];
        response.on('data', (b) => bytes.push(b));
        response.on('end', () => {
          let body = Buffer.concat(bytes);
          if (req.url.endsWith('/membership/check') && response.statusCode === 200) {
            const observation = JSON.parse(body);
            fault.observations.push(observation);
            if (fault.read === 'invalid') { observation.sig = '0'.repeat(128); body = Buffer.from(JSON.stringify(observation)); }
            if (fault.read === 'replay') body = Buffer.from(JSON.stringify(fault.observations[0]));
            if (fault.read === 'hold') { fault.held.push(() => res.writeHead(response.statusCode).end(body)); return; }
          }
          res.writeHead(response.statusCode, { 'content-type': 'application/json' }).end(body);
        });
      });
      upstream.on('error', () => res.writeHead(503).end());
      upstream.end(Buffer.concat(chunks));
    });
  });
  const wss = new WebSocketServer({ server });
  wss.on('connection', (client, req) => {
    const upstream = new ProxySocket(`ws://127.0.0.1:${targetPort}`, { headers: { Host: req.headers.host } });
    const queued = [];
    upstream.on('open', () => queued.forEach((m) => upstream.send(m)));
    client.on('message', (bytes) => {
      const data = String(bytes), frame = JSON.parse(data);
      if (frame[0] === 'EVENT' && [9030, 9031].includes(frame[1].kind)) fault.commands.push(frame[1]);
      if (upstream.readyState === ProxySocket.OPEN) upstream.send(data); else queued.push(data);
    });
    upstream.on('message', (bytes) => {
      const m = JSON.parse(String(bytes)), command = fault.commands.find((c) => c.id === m[1]);
      if (m[0] === 'OK' && m[2] === true && command) {
        fault.afterAck?.(command);
        if (command.tags.some((t) => t[0] === 'p' && t[1] === fault.dropAckFor)) {
          fault.dropAckFor = null; fault.dropped.push(command); client.terminate(); upstream.terminate(); return;
        }
      }
      if (client.readyState === ProxySocket.OPEN) client.send(String(bytes));
    });
    client.on('close', () => upstream.terminate());
    upstream.on('close', () => client.close());
    upstream.on('error', () => client.close());
    client.on('error', () => upstream.terminate());
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  servers.push({ server, wss });
  return server.address().port;
}
const backendBinary = resolve(process.env.REPLACEMENT_E2E_BACKEND), buzzBinary = resolve(process.env.RUSTSHARE_TEST_BUZZ_BINARY);
let backend, backendEnv, relayProcess, relayEnv, db, buzz;
const manifest = { elembra: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), buzz_source: execFileSync('git', ['-C', process.env.RUSTSHARE_TEST_BUZZ_SOURCE_DIR, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), buzz_executable_sha256: hash(readFileSync(buzzBinary)), results };
assert.equal(manifest.buzz_source, '2a4a8d926d79b406b7058419ef84c319a0c107fd');
assert.equal(manifest.buzz_executable_sha256, process.env.RUSTSHARE_TEST_BUZZ_BINARY_SHA256);
async function validate(name, args, cwd = root, extra = {}) {
  const fd = openSync(`${artifacts}/${name}.log`, 'w', 0o600);
  const env = { ...process.env, ...backendEnv, SQLX_OFFLINE: 'true', RUSTSHARE_TEST_DISPOSABLE_DB: '1', ...extra };
  if (extra.RUSTUP_TOOLCHAIN === '') delete env.RUSTUP_TOOLCHAIN;
  const child = spawn('cargo', args, { cwd, env, stdio: ['ignore', fd, fd] });
  children.push(child);
  const code = await new Promise((r) => { child.on('exit', r); child.on('error', () => r(-1)); });
  manifest.validation ??= [];
  manifest.validation.push({ name, command: ['cargo', ...args], status: code === 0 ? 'passed' : 'failed', exit_code: code });
  if (code !== 0) process.exitCode = 1;
  console.log(`${code === 0 ? 'PASS' : 'FAIL'} ${name}`);
}
try {
  const pgEnv = { POSTGRES_USER: 'rustshare', POSTGRES_PASSWORD: password, POSTGRES_DB: 'rustshare_test' };
  db = await dependency('elembra-pg', 'pgvector/pgvector@sha256:ccc6e83d6e35e931dc7c5def2022729d5a6c370318d099181995567ff1fb4d6b', [5432], pgEnv);
  buzz = await dependency('buzz-pg', 'postgres@sha256:9e2cf6287f25c3151f4c9421ccce2b17f3e60061c111a29165e0d2cdd9ff0a97', [5432], pgEnv);
  const redis = await dependency('redis', 'redis@sha256:e7723ff73d963f5cc6d9c4643ea3d989527a402a319239054e9472a7fb9219a2', [6379], {});
  const s3key = secret();
  const s3 = await dependency('rustfs', 'rustfs/rustfs@sha256:60f4f2f41ce95216f8cac676e69f9d90c0bfec458a3bc7fd7fb9b7c2452ac57a', [9000], { RUSTFS_ROOT_USER: 'replacement-test', RUSTFS_ROOT_PASSWORD: s3key, RUSTFS_DATA_DIR: '/data', RUSTFS_LOGS_DIR: '/logs' });
  for (const pg of [db, buzz]) await until('PostgreSQL TCP ready', async () => { try { return docker('exec', pg.container, 'pg_isready', '-h', '127.0.0.1', '-U', 'rustshare').includes('accepting'); } catch { return false; } });
  const actualRelayPort = await freePort(), relayPort = await proxy(actualRelayPort), apiPort = await freePort();
  const relayUrl = `ws://localhost:${relayPort}`, relayBase = `http://localhost:${relayPort}`, base = `http://127.0.0.1:${apiPort}`;
  const s3Endpoint = `http://127.0.0.1:${s3.port(9000)}`;
  relayEnv = { DATABASE_URL: `postgres://rustshare:${password}@127.0.0.1:${buzz.port(5432)}/rustshare_test`, REDIS_URL: `redis://127.0.0.1:${redis.port(6379)}`, RELAY_URL: relayUrl, BUZZ_BIND_ADDR: `127.0.0.1:${actualRelayPort}`, BUZZ_HEALTH_PORT: String(await freePort()), BUZZ_METRICS_PORT: String(await freePort()), BUZZ_AUTO_MIGRATE: 'true', BUZZ_GIT_ENABLED: 'false', BUZZ_RELAY_PRIVATE_KEY: relay.sk, RELAY_OWNER_PUBKEY: service.pk, RELAY_TRUSTED_SERVICE_PUBKEYS: service.pk, BUZZ_REQUIRE_AUTH_TOKEN: 'true', BUZZ_REQUIRE_RELAY_MEMBERSHIP: 'true', BUZZ_ALLOW_NIP_OA_AUTH: 'false', BUZZ_RATE_LIMIT_HUMAN_API_CALLS_PER_MIN: '10000', BUZZ_S3_ENDPOINT: s3Endpoint, BUZZ_S3_ACCESS_KEY: 'replacement-test', BUZZ_S3_SECRET_KEY: s3key, BUZZ_S3_BUCKET: 'replacement-e2e', RUST_LOG: 'info' };
  relayProcess = start(buzzBinary, relayEnv, 'buzz');
  await until('Buzz health', async () => { assert.equal(relayProcess.exitCode, null, 'Buzz exited; inspect buzz.log'); try { return (await fetch(`${relayBase}/health`)).ok; } catch { return false; } });
  manifest.buzz_schema = sql(buzz.container, 'SELECT max(version) FROM _sqlx_migrations WHERE success');
  assert.equal(manifest.buzz_schema, '29');
  const community = sql(buzz.container, `SELECT id FROM communities WHERE host=${quote(`localhost:${relayPort}`)}`);
  assert.match(community, /^[a-f0-9-]{36}$/);
  // Accepted Buzz setup mechanism; no membership mutation is simulated in SQL.
  sql(buzz.container, `UPDATE communities SET membership_v1alpha2=true WHERE id=${quote(community)}`);
  const encryptionKey = Buffer.from(secret(), 'hex').toString('base64'); secrets.push(encryptionKey);
  backendEnv = { DATABASE_URL: `postgres://rustshare:${password}@127.0.0.1:${db.port(5432)}/rustshare_test`, JWT_SECRET: secret(), RUSTSHARE_SECRET_ENCRYPTION_KEY: encryptionKey, RUSTSHARE_CHAT_WEBHOOK_SECRET: secret(), RUSTFS_ENDPOINT: s3Endpoint, RUSTFS_REGION: 'us-east-1', RUSTFS_BUCKET: 'replacement-e2e', AWS_ACCESS_KEY_ID: 'replacement-test', AWS_SECRET_ACCESS_KEY: s3key, RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET: 'true', RUSTSHARE_ADMIN_USERNAME: 'replacement-admin', RUSTSHARE_ADMIN_EMAIL: 'admin@replacement.test', RUSTSHARE_ADMIN_PASSWORD: password, PASSWORD_LOGIN_ENABLED: 'true', SESSION_COOKIE_SECURE: 'false', SERVER_HOST: '127.0.0.1', SERVER_PORT: String(apiPort), RUSTSHARE_PUBLIC_URL: base, RUSTSHARE_AI_ENABLED: 'false', RUSTSHARE_CHAT_AUTHORITY: 'buzz', RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: service.sk, RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY: 'true', RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL: relayUrl, RUSTSHARE_CHAT_PROVISIONING: 'manual', RUSTSHARE_CHAT_REPLACEMENT_INITIATION_ENABLED: '1', RUSTSHARE_CHAT_REPLACEMENT_WORKER_ENABLED: '1', RUSTSHARE_OUTBOX_WORKER_ENABLED: 'true', RUSTSHARE_OUTBOX_POLL_INTERVAL_MS: '100', RUSTSHARE_RATE_LIMIT_AUTH_LOGIN_PER_MINUTE: '200', RUST_LOG: 'info,rustshare_server::buzz_bridge=debug', ...Object.fromEntries(['RUSTSHARE_TEST_BUZZ_SOURCE_DIR', 'RUSTSHARE_TEST_BUZZ_TARGET_DIR', 'RUSTSHARE_TEST_BUZZ_BINARY'].map((k) => [k, process.env[k]])) };
  async function backendReady() { await until('Elembra health and migrations', async () => { assert.equal(backend.exitCode, null, 'Elembra exited; inspect elembra.log'); try { return (await fetch(`${base}/health`)).ok; } catch { return false; } }, 120_000); }
  backend = start(backendBinary, backendEnv, 'elembra'); await backendReady();
  pass('disposable backend, outbox, replacement worker, Buzz schema 29, two PostgreSQL, Redis, RustFS');
  const admin = await api(base, null, 'POST', '/auth/login', { email: 'admin@replacement.test', password });
  const tenant = admin.user.tenant_id;
  await api(base, admin, 'POST', '/admin/applications/io.elembra.chat/enable');
  await api(base, admin, 'POST', `/admin/applications/chat/workspaces/${tenant}/community`, { community_id: community, relay_url: relayUrl, relay_pubkey: relay.pk, membership_protocol_version: 'v1alpha2' }, 201);
  const status = (op) => sql(db.container, `SELECT status FROM chat_identity_bindings WHERE binding_id=${quote(op)}`);
  async function user(name) {
    const created = await api(base, admin, 'POST', '/admin/users', { username: name, email: `${name}@replacement.test`, password, display_name: name, storage_quota_bytes: 1024 }, 201);
    const session = await api(base, null, 'POST', '/auth/login', { email: `${name}@replacement.test`, password });
    assert.equal(session.user.id, created.id); return session;
  }
  async function challenge(session, signer, old) {
    const c = await api(base, session, 'POST', '/applications/chat/identity-binding/challenge', { workspace_id: tenant, buzz_pubkey: signer.pk, ...(old ? { rotation_of: old.binding_id } : {}) }, 201);
    return { c, proof: event(22242, [['relay', c.relay_url], ['challenge', c.nonce]], '', signer) };
  }
  async function onboard(name) {
    const session = await user(name), old = key(), candidate = key();
    const { c, proof } = await challenge(session, old);
    const binding = await api(base, session, 'POST', '/applications/chat/identity-binding/verify', { challenge_id: c.challenge_id, event: proof }, 201);
    await api(base, session, 'POST', '/applications/chat/admission', { workspace_id: tenant }, 202);
    await until('old relay membership active', async () => (await membership(relayBase, community, old.pk)).membership === 'active');
    assert.equal((await membership(relayBase, community, candidate.pk)).membership, 'absent');
    return { session, old, candidate, binding };
  }
  async function replacement(person) {
    const { c, proof } = await challenge(person.session, person.candidate, person.binding);
    return { workspace_id: tenant, challenge_id: c.challenge_id, old_binding_id: person.binding.binding_id, event: proof, operation_id: randomUUID() };
  }
  const initiate = (person, request) => api(base, person.session, 'POST', '/applications/chat/identity-replacements', request, 202);
  const operationStatus = (person, op) => api(base, person.session, 'GET', `/applications/chat/identity-replacements/${op}`);
  async function completed(person, op) {
    await until('durable replacement completion', async () => (await operationStatus(person, op)).state === 'completed');
    assert.equal((await membership(relayBase, community, person.old.pk)).membership, 'revoked');
    assert.equal((await membership(relayBase, community, person.candidate.pk)).membership, 'active');
    assert.equal(status(op), 'active');
  }
  const alice = await onboard('alice'); pass('first-time v1alpha2 HTTP admission through real outbox');
  const channel = randomUUID();
  assert.equal((await wire(relayUrl, service, event(9007, [['h', channel], ['name', 'acceptance'], ['visibility', 'open'], ['channel_type', 'stream']], '', service)))[2], true);
  const historical = event(9, [['h', channel]], 'historical attribution', alice.old);
  assert.equal((await wire(relayUrl, alice.old, historical))[2], true);
  assert.equal((await wire(relayUrl, alice.candidate, event(9, [['h', channel]], 'must deny', alice.candidate)))[2], false);
  pass('normal message delivery; absent candidate cannot send');
  const request = await replacement(alice), operation = request.operation_id;
  await stop(relayProcess);
  assert.equal((await initiate(alice, request)).operation_id, operation);
  assert.equal(status(operation), 'pending'); assert.equal(status(alice.binding.binding_id), 'revoked');
  assert.equal(sql(db.container, `SELECT count(*) FROM chat_buzz_admissions WHERE binding_id=${quote(alice.binding.binding_id)} AND active`), '0');
  await until('worker recorded relay unavailable', async () => Number(sql(db.container, `SELECT transient_failures FROM chat_identity_replacements WHERE operation_id=${quote(operation)}`)) > 0);
  assert.equal(status(operation), 'pending'); pass('relay unavailable: local revocation, pending candidate and durable retry');
  await stop(backend); backend = start(backendBinary, backendEnv, 'elembra'); await backendReady();
  relayProcess = start(buzzBinary, relayEnv, 'buzz');
  await completed(alice, operation);
  const commands = rows(db.container, `SELECT command_id,phase,attempt,outcome,expected_revision FROM chat_identity_replacement_commands WHERE operation_id=${quote(operation)} ORDER BY created_at`);
  assert.deepEqual(commands.map((v) => [v.phase, v.outcome]), [['revoke', 'delivered'], ['admit', 'delivered']]);
  const observations = rows(db.container, `SELECT pubkey,accepted_state,accepted_event_id,request_id,accepted_revision FROM chat_buzz_membership_verifications WHERE operation_id=${quote(operation)} AND accepted_at IS NOT NULL ORDER BY requested_at`);
  for (const [pk, state] of [[alice.old.pk, 'revoked'], [alice.candidate.pk, 'absent'], [alice.candidate.pk, 'active']]) assert(observations.some((o) => o.pubkey === pk && o.accepted_state === state));
  assert.equal(new Set(observations.map((o) => o.request_id)).size, observations.length);
  assert(observations.every((o) => /^[a-f0-9]{64}$/.test(o.accepted_event_id)));
  const audit = rows(db.container, `SELECT tenant_id,actor_id,from_state,to_state,reason_code FROM chat_identity_replacement_audit WHERE operation_id=${quote(operation)} ORDER BY created_at`);
  assert(audit.every((a) => a.tenant_id === tenant)); assert(audit.some((a) => a.to_state === 'completed'));
  manifest.happy_path = { operation, commands, observations, audit, historical_event_id: historical.id, historical_author: historical.pubkey };
  pass('complete HTTP → NIP-43 revoke → signed REVOKED/ABSENT → admit → signed ACTIVE → atomic local completion, after restart');
  assert.equal((await wire(relayUrl, alice.old, event(9, [['h', channel]], 'old denied', alice.old)))[2], false);
  assert.equal((await wire(relayUrl, alice.candidate, event(9, [['h', channel]], 'new authorized', alice.candidate)))[2], true);
  const oldState = await membership(relayBase, community, alice.old.pk);
  assert.equal((await wire(relayUrl, service, event(9030, [['p', alice.old.pk], ['elembra-op', randomUUID()], ['elembra-expected-revision', String(oldState.membership_revision)]], '', service)))[2], false);
  pass('revoked old key cannot reconnect, send or regain membership; completed candidate can send');
  await initiate(alice, request);
  assert.equal(sql(db.container, `SELECT count(*) FROM chat_identity_replacement_commands WHERE operation_id=${quote(operation)}`), '2');
  pass('exact initiation replay preserves durable operation and command IDs');
  const bob = await user('bob');
  await api(base, bob, 'GET', `/applications/chat/identity-replacements/${operation}`, undefined, 404);
  const foreignTenant = randomUUID();
  sql(db.container, `INSERT INTO tenants(id,name) VALUES (${quote(foreignTenant)},'isolated acceptance tenant'); INSERT INTO users(id,username,email,password_hash,display_name,storage_quota,tenant_id) SELECT gen_random_uuid(),'foreign','foreign@replacement.test',password_hash,'foreign',1024,${quote(foreignTenant)} FROM users WHERE id=${quote(admin.user.id)}`);
  const foreign = await api(base, null, 'POST', '/auth/login', { email: 'foreign@replacement.test', password });
  await api(base, foreign, 'GET', `/applications/chat/identity-replacements/${operation}`, undefined, 404);
  await api(base, foreign, 'POST', '/applications/chat/identity-replacements', request, 403);
  pass('wrong Principal and foreign tenant cannot read or initiate owner operation');
  const security = await onboard('security'), securityRequest = await replacement(security);
  for (const [name, override, expected] of [['wrong workspace', { workspace_id: foreignTenant }, 403], ['wrong old binding', { old_binding_id: randomUUID() }, 409], ['wrong candidate proof', { event: event(22242, [['relay', relayUrl], ['challenge', 'wrong']], '', key()) }, 400]]) {
    await api(base, security.session, 'POST', '/applications/chat/identity-replacements', { ...securityRequest, ...override }, expected); pass(`${name} rejected`);
  }
  assert.equal(status(security.binding.binding_id), 'active');
  sql(db.container, `UPDATE chat_binding_challenges SET expires_at=clock_timestamp()-interval '1 second' WHERE challenge_id=${quote(securityRequest.challenge_id)}`);
  await api(base, security.session, 'POST', '/applications/chat/identity-replacements', securityRequest, 410); pass('expired challenge rejected');
  const rollbackRequest = await replacement(security);
  sql(db.container, `CREATE FUNCTION replacement_e2e_fail_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'acceptance injected audit failure'; END $$; CREATE TRIGGER replacement_e2e_fail_audit BEFORE INSERT ON chat_identity_replacement_audit FOR EACH ROW EXECUTE FUNCTION replacement_e2e_fail_audit()`);
  try { await api(base, security.session, 'POST', '/applications/chat/identity-replacements', rollbackRequest, 500); }
  finally { sql(db.container, 'DROP TRIGGER replacement_e2e_fail_audit ON chat_identity_replacement_audit; DROP FUNCTION replacement_e2e_fail_audit()'); }
  assert.equal(status(security.binding.binding_id), 'active');
  assert.equal(sql(db.container, `SELECT count(*) FROM chat_identity_replacements WHERE operation_id=${quote(rollbackRequest.operation_id)}`), '0');
  assert.equal(sql(db.container, `SELECT consumed_at IS NULL FROM chat_binding_challenges WHERE challenge_id=${quote(rollbackRequest.challenge_id)}`), 't');
  pass('audit transaction failure rolls back challenge, bindings, operation and outbox');
  const revoked = await onboard('revoked-candidate'), revokedState = await membership(relayBase, community, revoked.candidate.pk);
  assert.equal((await wire(relayUrl, service, event(9031, [['p', revoked.candidate.pk], ['elembra-op', randomUUID()], ['elembra-expected-revision', String(revokedState.membership_revision)]], '', service)))[2], true);
  const rejected = await replacement(revoked); await initiate(revoked, rejected);
  await until('revoked candidate blocked', async () => (await operationStatus(revoked, rejected.operation_id)).state === 'blocked');
  assert.equal(status(rejected.operation_id), 'pending');
  assert(Number(sql(db.container, `SELECT count(*) FROM chat_identity_replacement_audit WHERE operation_id=${quote(rejected.operation_id)} AND reason_code IS NOT NULL`)) > 0);
  pass('revoked candidate remains pending with audited permanent failure');
  const lost = await onboard('lost-ack'), lostRequest = await replacement(lost);
  fault.dropAckFor = lost.old.pk;
  await initiate(lost, lostRequest);
  await until('accepted command ACK dropped', async () => fault.dropped.some((c) => c.tags.some((t) => t[0] === 'p' && t[1] === lost.old.pk)));
  await completed(lost, lostRequest.operation_id);
  const dropped = fault.dropped.find((c) => c.tags.some((t) => t[0] === 'p' && t[1] === lost.old.pk));
  assert(fault.commands.filter((c) => c.id === dropped.id).length >= 2, 'ambiguous retry must preserve the exact signed event');
  assert.equal(sql(db.container, `SELECT count(*) FROM chat_identity_replacement_commands WHERE operation_id=${quote(lostRequest.operation_id)} AND phase='revoke'`), '1');
  assert.equal((await wire(relayUrl, service, dropped))[2], true);
  assert.equal((await membership(relayBase, community, lost.old.pk)).membership, 'revoked');
  pass('lost ACK, stable command UUID and exact signed retry; duplicate UUID does not reactivate old key');
  for (const mode of ['invalid', 'replay']) {
    const person = await onboard(`evidence-${mode}`), input = await replacement(person);
    fault.read = mode; await initiate(person, input);
    try { await until(`${mode} evidence blocked`, async () => (await operationStatus(person, input.operation_id)).state === 'blocked'); assert.equal(status(input.operation_id), 'pending'); }
    finally { fault.read = null; }
    pass(`${mode} signed membership evidence cannot activate candidate`);
  }
  // Hold the real final read after the real relay admission ACK.
  const staged = await onboard('ack-before-verification'), stagedRequest = await replacement(staged);
  fault.afterAck = (command) => { if (command.kind === 9030 && command.tags.some((t) => t[0] === 'p' && t[1] === staged.candidate.pk)) { fault.read = 'hold'; fault.afterAck = null; } };
  await initiate(staged, stagedRequest);
  await until('final signed read held', async () => fault.held.length > 0);
  assert.equal(status(stagedRequest.operation_id), 'pending');
  assert.equal((await operationStatus(staged, stagedRequest.operation_id)).state, 'pending_membership_verification');
  pass('relay OK alone leaves candidate pending and operation incomplete');
  const premature = await wire(relayUrl, staged.candidate, event(9, [['h', channel]], 'must deny before verified completion', staged.candidate));
  manifest.pre_activation_probe = { operation_id: stagedRequest.operation_id, local_status: 'pending', operation_state: 'pending_membership_verification', relay_response: premature };
  fault.read = null; fault.held.splice(0).forEach((release) => release());
  assert.equal(premature[2], false, 'candidate must not send while final signed evidence is withheld and local binding is pending');
  pass('candidate cannot send before verified completion');
} catch (error) {
  manifest.error = redact(error.stack); results.push({ name: 'acceptance', status: 'failed', error: manifest.error }); console.error(manifest.error); process.exitCode = 1;
  manifest.wire_commands = fault.commands;
  if (db) { try {
    manifest.failed_state = rows(db.container, 'SELECT operation_id,state,generation,transient_failures,failure_code,blocked_at FROM chat_identity_replacements');
    manifest.failed_commands = rows(db.container, 'SELECT command_id,phase,outcome,expected_revision FROM chat_identity_replacement_commands');
    manifest.deliveries = rows(db.container, 'SELECT * FROM integration_deliveries');
  } catch { /* Startup can fail before migrations. */ } }
} finally {
  for (const { server, wss } of servers) { for (const client of wss.clients) client.terminate(); server.closeAllConnections(); server.close(); }
  for (const child of children.toReversed()) await stop(child);
  if (process.env.REPLACEMENT_E2E_VALIDATE === '1' && backendEnv && !interrupted) {
    await validate('postgres-regressions', ['test', '-p', 'rustshare-server', '--test', 'chat_identity_lifecycle_test', '--all-features', '--', '--ignored', '--test-threads=1']);
    await validate('openapi', ['test', '-p', 'rustshare-server', '--test', 'openapi_export_test', '--all-features']);
    await validate('sqlx-metadata', ['sqlx', 'prepare', '--workspace', '--check'], root, { SQLX_OFFLINE: 'false' });
    const upstream = { BUZZ_TEST_DATABASE_URL: relayEnv.DATABASE_URL, BUZZ_TEST_REDIS_URL: relayEnv.REDIS_URL, BUZZ_TEST_RELAY_BINARY: buzzBinary, BUZZ_TEST_POSTGRES_CONTAINER: buzz.container, CARGO_TARGET_DIR: process.env.RUSTSHARE_TEST_BUZZ_TARGET_DIR, RUSTUP_TOOLCHAIN: '' };
    await validate('buzz-migration-29', ['test', '-p', 'buzz-db', '--test', 'membership_v1alpha2', '--', '--ignored'], process.env.RUSTSHARE_TEST_BUZZ_SOURCE_DIR, upstream);
    await validate('buzz-authoritative-live', ['test', '-p', 'buzz-test-client', '--test', 'authoritative_membership', '--', '--ignored', '--nocapture'], process.env.RUSTSHARE_TEST_BUZZ_SOURCE_DIR, upstream);
  }
  for (const container of containers.toReversed()) {
    try { writeFileSync(`${artifacts}/${container}.log`, redact(execFileSync('docker', ['logs', '--tail', '150', container], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }))); } catch { /* Container may not have started. */ }
    try { docker('rm', '-f', '-v', container); } catch { process.exitCode = 1; }
  }
  for (const file of readdirSync(artifacts).filter((f) => f.endsWith('.log'))) {
    writeFileSync(`${artifacts}/${file}`, redact(readFileSync(`${artifacts}/${file}`, 'utf8').split('\n').slice(-250).join('\n')).slice(-64_000));
  }
  writeFileSync(`${artifacts}/results.json`, JSON.stringify(manifest, null, 2));
  console.log(`Evidence: ${artifacts}/results.json`);
}
