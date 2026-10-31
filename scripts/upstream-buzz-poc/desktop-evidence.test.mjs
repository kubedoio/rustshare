import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const collector = fileURLToPath(new URL('./desktop-evidence.mjs', import.meta.url));
const alice = '11'.repeat(32);
const bob = '22'.repeat(32);
const message = (direction, channel, id, author, target) => ({
	direction,
	channel_id: channel,
	observed_channel_id: channel,
	event_id: id,
	observed_event_id: id,
	author_pubkey: author,
	observed_author_pubkey: author,
	visible_in_desktop: target === 'desktop',
	observed_by_protocol: target === 'protocol',
});
const input = () => {
	const channel = '11111111-1111-4111-8111-111111111111';
	const dm = '22222222-2222-4222-8222-222222222222';
	return {
		relay_url: 'wss://relay.example',
		expected_pubkey: alice,
		displayed_pubkey: alice,
		expected_peer_pubkey: bob,
		expected_channel_id: channel,
		expected_dm_channel_id: dm,
		checks: { nsec_imported: true, intended_community_joined: true, identity_persisted_after_restart: true, reconnected_after_restart: true, history_restored: true, no_duplicate_identity: true, no_unexpected_enrollment: true, private_dm_discovered: true },
		events: [message('protocol_to_desktop', channel, 'aa'.repeat(32), alice, 'desktop'), message('desktop_to_protocol', channel, 'bb'.repeat(32), alice, 'protocol')],
		dm_events: [message('protocol_to_desktop', dm, 'cc'.repeat(32), bob, 'desktop'), message('desktop_to_protocol', dm, 'dd'.repeat(32), alice, 'protocol')],
	};
};
const run = (value) => spawnSync(process.execPath, [collector], { input: value, encoding: 'utf8' });

test('emits only correlated public and private-channel observations', () => {
	const result = run(JSON.stringify(input()));
	assert.equal(result.status, 0);
	const output = JSON.parse(result.stdout);
	assert.equal(output.status, 'PASS_REVIEW_REQUIRED');
	assert.equal(output.checks.private_dm_bidirectional, true);
	assert.equal(output.dm_events.length, 2);
});

test('rejects credential-bearing relay URLs without echoing credentials', () => {
	const data = input();
	data.relay_url = 'wss://user:sentinel-password@relay.example/?token=secret';
	const result = run(JSON.stringify(data));
	assert.equal(result.status, 2);
	assert.equal(result.stderr.includes('sentinel-password'), false);
	assert.equal(result.stderr.includes('token=secret'), false);
});

test('malformed input does not leak parser excerpts', () => {
	const result = run('sentinel-nsec-value');
	assert.equal(result.status, 2);
	assert.equal(result.stderr.includes('sentinel-nsec-value'), false);
});

test('marks DM events unverified when the observed channel differs', () => {
	const data = input();
	data.dm_events[0].observed_channel_id = '33333333-3333-4333-8333-333333333333';
	const result = run(JSON.stringify(data));
	assert.equal(result.status, 0);
	assert.equal(JSON.parse(result.stdout).checks.private_dm_bidirectional, false);
});

test('rejects identical identities or channel IDs for the DM proof', () => {
	const data = input();
	data.expected_peer_pubkey = data.expected_pubkey;
	assert.equal(run(JSON.stringify(data)).status, 2);
	data.expected_peer_pubkey = bob;
	data.expected_dm_channel_id = data.expected_channel_id;
	assert.equal(run(JSON.stringify(data)).status, 2);
});

test('rejects an event reused as proof in both public and DM channels', () => {
	const data = input();
	data.dm_events[0].event_id = data.events[0].event_id;
	data.dm_events[0].observed_event_id = data.events[0].observed_event_id;
	assert.equal(run(JSON.stringify(data)).status, 2);
});
