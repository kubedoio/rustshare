import assert from 'node:assert/strict';
import test from 'node:test';
import { isUnboundAuthorRejection } from './buzz-observer-policy.mjs';

test('only an explicit unbound-author 403 is terminal during replay', () => {
	assert.equal(isUnboundAuthorRejection(403, { error: 'Unbound author' }), true);
	assert.equal(isUnboundAuthorRejection(403, { error: 'Event verification failed' }), false);
	assert.equal(isUnboundAuthorRejection(403, { error: 'Unknown community' }), false);
	assert.equal(isUnboundAuthorRejection(500, { error: 'Unbound author' }), false);
});
