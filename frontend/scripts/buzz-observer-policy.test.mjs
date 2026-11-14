import assert from 'node:assert/strict';
import test from 'node:test';
import {
	completeObserverReconciliation,
	isUnboundAuthorRejection,
	markObserverFailure,
	observerIsReady
} from './buzz-observer-policy.mjs';

test('only an explicit unbound-author 403 is terminal during replay', () => {
	assert.equal(isUnboundAuthorRejection(403, { error: 'Unbound author' }), true);
	assert.equal(isUnboundAuthorRejection(403, { error: 'Event verification failed' }), false);
	assert.equal(isUnboundAuthorRejection(403, { error: 'Unknown community' }), false);
	assert.equal(isUnboundAuthorRejection(500, { error: 'Unbound author' }), false);
});

test('observer readiness requires successful history reconciliation', () => {
	const state = {
		connection: 'connected',
		auth: 'authenticated',
		registry: 'ready',
		community: 'community-id',
		observation: 'starting',
		changedCommunity: false
	};
	assert.equal(observerIsReady(state), false);
	state.observation = 'ready';
	assert.equal(observerIsReady(state), true);
	state.observation = 'degraded';
	assert.equal(observerIsReady(state), false);
});

test('observer reconciliation only clears failures it observed and replayed', () => {
	const state = {
		connection: 'connected',
		auth: 'authenticated',
		registry: 'ready',
		community: 'community-id',
		observation: 'starting',
		failureGeneration: 0,
		changedCommunity: false
	};
	assert.equal(observerIsReady(state), false);
	assert.equal(completeObserverReconciliation(state, 0), true);
	assert.equal(observerIsReady(state), true);

	const staleReplay = state.failureGeneration;
	markObserverFailure(state);
	assert.equal(observerIsReady(state), false);
	assert.equal(completeObserverReconciliation(state, staleReplay), false);
	assert.equal(observerIsReady(state), false);

	const recoveryReplay = state.failureGeneration;
	assert.equal(completeObserverReconciliation(state, recoveryReplay), true);
	assert.equal(observerIsReady(state), true);
});
