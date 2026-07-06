import { writable } from 'svelte/store';

export type ReplicationState =
	'primary_written' | 'queued' | 'syncing' | 'fully_replicated' | 'degraded' | 'failed';

export interface ReplicationStatus {
	fileId: string;
	fileVersionId: string;
	replicationState: ReplicationState;
	jobStatus: string | null;
	attemptCount: number;
	nextAttemptAt: string | null;
	lastError: string | null;
	updatedAt: string;
}

type ReplicationStateMap = Record<string, ReplicationStatus>;

function createReplicationStore() {
	const { subscribe, set, update } = writable<ReplicationStateMap>({});

	return {
		subscribe,
		upsert: (status: ReplicationStatus) => {
			update((states) => ({
				...states,
				[status.fileId]: status
			}));
		},
		remove: (fileId: string) => {
			update((states) => {
				const nextStates = { ...states };
				delete nextStates[fileId];
				return nextStates;
			});
		},
		reset: () => {
			set({});
		}
	};
}

export const replicationStore = createReplicationStore();

export function formatReplicationStateLabel(state: ReplicationState): string {
	switch (state) {
		case 'primary_written':
			return 'Primary saved';
		case 'queued':
			return 'Queued';
		case 'syncing':
			return 'Replicating';
		case 'fully_replicated':
			return 'Replicated';
		case 'degraded':
			return 'Retrying';
		case 'failed':
			return 'Replication failed';
	}
}

export function replicationStateBadgeClass(state: ReplicationState): string {
	switch (state) {
		case 'primary_written':
			return 'badge-ghost';
		case 'queued':
			return 'badge-info';
		case 'syncing':
			return 'badge-info';
		case 'fully_replicated':
			return 'badge-success';
		case 'degraded':
			return 'badge-warning';
		case 'failed':
			return 'badge-error';
	}
}
