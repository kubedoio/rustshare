#!/usr/bin/env node
import { readFileSync } from 'node:fs';

const topKeys = new Set(['relay_url', 'expected_pubkey', 'displayed_pubkey', 'expected_peer_pubkey', 'expected_channel_id', 'expected_dm_channel_id', 'checks', 'events', 'dm_events']);
const checkKeys = new Set([
	'nsec_imported', 'intended_community_joined', 'identity_persisted_after_restart',
	'reconnected_after_restart', 'history_restored', 'no_duplicate_identity',
	'no_unexpected_enrollment', 'private_dm_discovered', 'credential_removed_after_test',
]);
const directions = new Set(['protocol_to_desktop', 'desktop_to_protocol']);

function exactKeys(value, allowed) {
	if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error();
	if (Object.keys(value).some((key) => !allowed.has(key))) throw new Error();
}

function hex(value, length) {
	if (typeof value !== 'string' || !new RegExp(`^[0-9a-f]{${length}}$`).test(value)) throw new Error();
	return value;
}

function uuid(value) {
	if (typeof value !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value)) throw new Error();
	return value;
}

function eventPair(items, channelId) {
	if (!Array.isArray(items) || items.length !== 2) throw new Error();
	const parsed = items.map((item) => {
		exactKeys(item, new Set(['direction', 'channel_id', 'observed_channel_id', 'event_id', 'observed_event_id', 'author_pubkey', 'observed_author_pubkey', 'visible_in_desktop', 'observed_by_protocol']));
		if (!directions.has(item.direction)) throw new Error();
		return {
			direction: item.direction,
			channel_id: uuid(item.channel_id),
			observed_channel_id: uuid(item.observed_channel_id),
			event_id: hex(item.event_id, 64),
			observed_event_id: hex(item.observed_event_id, 64),
			author_pubkey: hex(item.author_pubkey, 64),
			observed_author_pubkey: hex(item.observed_author_pubkey, 64),
			visible_in_desktop: item.visible_in_desktop === true,
			observed_by_protocol: item.observed_by_protocol === true,
		};
	});
	if (new Set(parsed.map((item) => item.direction)).size !== 2) throw new Error();
	if (parsed[0].event_id === parsed[1].event_id) throw new Error();
	return parsed.map((item) => ({
		...item,
		verified: item.channel_id === channelId
			&& item.observed_channel_id === channelId
			&& item.event_id === item.observed_event_id
			&& item.author_pubkey === item.observed_author_pubkey
			&& (item.direction === 'protocol_to_desktop' ? item.visible_in_desktop : item.observed_by_protocol),
	}));
}

try {
	let input;
	try { input = JSON.parse(readFileSync(0, 'utf8')); } catch { throw new Error(); }
	exactKeys(input, topKeys);
	let relay;
	try { relay = new URL(input.relay_url); } catch { throw new Error(); }
	if (relay.protocol !== 'wss:' || relay.username || relay.password || relay.pathname !== '/' || relay.search || relay.hash) throw new Error();
	const expectedPubkey = hex(input.expected_pubkey, 64);
	const displayedPubkey = hex(input.displayed_pubkey, 64);
	const expectedPeerPubkey = hex(input.expected_peer_pubkey, 64);
	const channelId = uuid(input.expected_channel_id);
	const dmChannelId = uuid(input.expected_dm_channel_id);
	if (expectedPubkey === expectedPeerPubkey || channelId === dmChannelId) throw new Error();
	exactKeys(input.checks, checkKeys);
	const checks = Object.fromEntries([...checkKeys].map((key) => [key, input.checks[key] === true]));
	const events = eventPair(input.events, channelId);
	const dmEvents = eventPair(input.dm_events, dmChannelId);
	if (new Set([...events, ...dmEvents].map((item) => item.event_id)).size !== 4) throw new Error();
	checks.public_key_matches = expectedPubkey === displayedPubkey;
	checks.protocol_to_desktop_author_matches = events.find((item) => item.direction === 'protocol_to_desktop')?.author_pubkey === expectedPubkey;
	checks.desktop_to_protocol_author_matches = events.find((item) => item.direction === 'desktop_to_protocol')?.author_pubkey === expectedPubkey;
	checks.protocol_to_desktop_event_matches = events.find((item) => item.direction === 'protocol_to_desktop')?.verified === true;
	checks.desktop_to_protocol_event_matches = events.find((item) => item.direction === 'desktop_to_protocol')?.verified === true;
	checks.private_dm_bidirectional = dmEvents.every((item) => item.verified);
	checks.private_dm_sender_matches = dmEvents.find((item) => item.direction === 'protocol_to_desktop')?.author_pubkey === expectedPeerPubkey
		&& dmEvents.find((item) => item.direction === 'desktop_to_protocol')?.author_pubkey === expectedPubkey;
	const result = {
		status: Object.values(checks).every(Boolean) ? 'PASS_REVIEW_REQUIRED' : 'UNVERIFIED_OR_FAIL',
		relay_url: relay.origin,
		public_key: expectedPubkey,
		checks,
		events,
		dm_events: dmEvents,
	};
	process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} catch {
	process.stderr.write('Rejected desktop evidence input; no input values were emitted.\n');
	process.exitCode = 2;
}
