import assert from 'node:assert/strict';
import test from 'node:test';
import { channelRef } from './channel-tags.mjs';

test('accepts exactly one well-formed h tag', () => {
	assert.deepEqual(channelRef({ tags: [['h', 'channel-1']] }), {
		tags: [['h', 'channel-1']], valid: true, channel_id: 'channel-1',
	});
});

test('rejects missing, duplicate, malformed, and non-string channel references', () => {
	for (const tags of [undefined, [], [['h', 'one'], ['h', 'two']], ['hX'], [['h']], [['h', 7]]]) {
		assert.equal(channelRef({ tags }).valid, false);
		assert.equal(channelRef({ tags }).channel_id, null);
	}
});

test('ignores a non-tag value when one valid h tag is present', () => {
	assert.equal(channelRef({ tags: [['h', 'channel-1'], 'hX'] }).channel_id, 'channel-1');
});
