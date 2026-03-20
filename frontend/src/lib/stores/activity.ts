import { writable } from 'svelte/store';

export type ActivityType =
	| 'file_uploaded'
	| 'file_modified'
	| 'file_downloaded'
	| 'file_deleted'
	| 'file_renamed'
	| 'file_moved'
	| 'folder_created'
	| 'folder_deleted'
	| 'folder_renamed'
	| 'folder_moved'
	| 'share_created'
	| 'share_revoked';

export interface Activity {
	id: string;
	type: ActivityType;
	fileName: string;
	timestamp: string; // ISO 8601
	details?: string; // Additional context
}

const MAX_ACTIVITIES = 50;
const STORAGE_KEY = 'activity-history';

// Load from localStorage
function loadActivities(): Activity[] {
	if (typeof window === 'undefined') return [];

	try {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (stored) {
			const activities = JSON.parse(stored);
			return Array.isArray(activities) ? activities.slice(0, MAX_ACTIVITIES) : [];
		}
	} catch (error) {
		console.error('Failed to load activity history:', error);
	}

	return [];
}

// Save to localStorage
function saveActivities(activities: Activity[]) {
	if (typeof window === 'undefined') return;

	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(activities));
	} catch (error) {
		console.error('Failed to save activity history:', error);
	}
}

function createActivityStore() {
	const { subscribe, set, update } = writable<Activity[]>(loadActivities());

	return {
		subscribe,

		addActivity: (type: ActivityType, fileName: string, details?: string) => {
			update((activities) => {
				const newActivity: Activity = {
					id: crypto.randomUUID(),
					type,
					fileName,
					timestamp: new Date().toISOString(),
					details
				};

				// Add new activity at the beginning
				const updated = [newActivity, ...activities].slice(0, MAX_ACTIVITIES);
				saveActivities(updated);
				return updated;
			});
		},

		clearHistory: () => {
			set([]);
			saveActivities([]);
		},

		removeActivity: (id: string) => {
			update((activities) => {
				const updated = activities.filter((a) => a.id !== id);
				saveActivities(updated);
				return updated;
			});
		}
	};
}

export const activityStore = createActivityStore();

// Helper function to get activity display info
export function getActivityDisplay(activity: Activity): {
	icon: string;
	title: string;
	description: string;
	color: string;
} {
	switch (activity.type) {
		case 'file_uploaded':
			return {
				icon: '📤',
				title: 'File Uploaded',
				description: `Uploaded ${activity.fileName}`,
				color: 'text-success'
			};
		case 'file_downloaded':
			return {
				icon: '📥',
				title: 'File Downloaded',
				description: `Downloaded ${activity.fileName}`,
				color: 'text-info'
			};
		case 'file_modified':
			return {
				icon: '♻️',
				title: 'File Modified',
				description: `Modified ${activity.fileName}`,
				color: 'text-info'
			};
		case 'file_deleted':
			return {
				icon: '🗑️',
				title: 'File Deleted',
				description: `Deleted ${activity.fileName}`,
				color: 'text-error'
			};
		case 'file_renamed':
			return {
				icon: '✏️',
				title: 'File Renamed',
				description: activity.details
					? `Renamed "${activity.details}" to "${activity.fileName}"`
					: `Renamed to ${activity.fileName}`,
				color: 'text-warning'
			};
		case 'file_moved':
			return {
				icon: '📂',
				title: 'File Moved',
				description: activity.details
					? `Moved ${activity.fileName} to ${activity.details}`
					: `Moved ${activity.fileName}`,
				color: 'text-info'
			};
		case 'folder_created':
			return {
				icon: '📁',
				title: 'Folder Created',
				description: `Created folder ${activity.fileName}`,
				color: 'text-success'
			};
		case 'folder_deleted':
			return {
				icon: '🗑️',
				title: 'Folder Deleted',
				description: `Deleted folder ${activity.fileName}`,
				color: 'text-error'
			};
		case 'folder_renamed':
			return {
				icon: '✏️',
				title: 'Folder Renamed',
				description: activity.details
					? `Renamed "${activity.details}" to "${activity.fileName}"`
					: `Renamed to ${activity.fileName}`,
				color: 'text-warning'
			};
		case 'folder_moved':
			return {
				icon: '📂',
				title: 'Folder Moved',
				description: activity.details
					? `Moved ${activity.fileName} to ${activity.details}`
					: `Moved ${activity.fileName}`,
				color: 'text-info'
			};
		case 'share_created':
			return {
				icon: '🔗',
				title: 'Share Link Created',
				description: `Created share link for ${activity.fileName}`,
				color: 'text-primary'
			};
		case 'share_revoked':
			return {
				icon: '🚫',
				title: 'Share Link Revoked',
				description: `Revoked share link for ${activity.fileName}`,
				color: 'text-warning'
			};
		default:
			return {
				icon: '📄',
				title: 'Activity',
				description: activity.fileName,
				color: 'text-base-content'
			};
	}
}

// Helper to format relative time
export function getRelativeTime(timestamp: string): string {
	const now = new Date().getTime();
	const then = new Date(timestamp).getTime();
	const diffMs = now - then;

	const seconds = Math.floor(diffMs / 1000);
	const minutes = Math.floor(seconds / 60);
	const hours = Math.floor(minutes / 60);
	const days = Math.floor(hours / 24);

	if (seconds < 60) return 'Just now';
	if (minutes < 60) return `${minutes}m ago`;
	if (hours < 24) return `${hours}h ago`;
	if (days < 7) return `${days}d ago`;

	return new Date(timestamp).toLocaleDateString('en-US', {
		month: 'short',
		day: 'numeric'
	});
}
