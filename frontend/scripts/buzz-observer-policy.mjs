export function isUnboundAuthorRejection(status, body) {
	return status === 403 && body?.error === 'Unbound author';
}

export function observerIsReady(state) {
	return (
		state.connection === 'connected' &&
		state.auth === 'authenticated' &&
		state.registry === 'ready' &&
		!!state.community &&
		state.observation === 'ready' &&
		!state.changedCommunity
	);
}

export function markObserverFailure(state) {
	state.failureGeneration += 1;
	state.observation = 'degraded';
}

export function completeObserverReconciliation(state, failureGeneration, since, recoveredAt) {
	if (state.failureGeneration !== failureGeneration) return false;
	state.observation = 'ready';
	state.lastReconcileSince = since;
	state.lastRecoveryAt = recoveredAt;
	return true;
}
