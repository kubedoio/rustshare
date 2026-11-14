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
		lastReconcileSince: 100,
		lastRecoveryAt: 'previous-recovery',
		changedCommunity: false
	};
	assert.equal(observerIsReady(state), false);
	assert.equal(completeObserverReconciliation(state, 0, 100, 'first-recovery'), true);
	assert.equal(observerIsReady(state), true);
	assert.equal(state.lastReconcileSince, 100);
	assert.equal(state.lastRecoveryAt, 'first-recovery');

	const staleReplay = state.failureGeneration;
	markObserverFailure(state);
	assert.equal(observerIsReady(state), false);
	assert.equal(completeObserverReconciliation(state, staleReplay, 200, 'stale-recovery'), false);
	assert.equal(observerIsReady(state), false);
	assert.equal(state.lastReconcileSince, 100);
	assert.equal(state.lastRecoveryAt, 'first-recovery');

	const recoveryReplay = state.failureGeneration;
	assert.equal(completeObserverReconciliation(state, recoveryReplay, 200, 'recovered'), true);
	assert.equal(observerIsReady(state), true);
	assert.equal(state.lastReconcileSince, 200);
	assert.equal(state.lastRecoveryAt, 'recovered');
});
