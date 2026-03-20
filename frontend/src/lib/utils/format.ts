export function formatFileSize(bytes: number): string {
	if (bytes === 0) return '0 Bytes';
	if (bytes < 1024) return `${bytes} Bytes`;
	const k = 1024;
	const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
	const i = Math.floor(Math.log(bytes) / Math.log(k));
	const value = bytes / Math.pow(k, i);
	return `${parseFloat(value.toFixed(2))} ${sizes[i]}`;
}

export function formatDate(dateString: string): string {
	const date = new Date(dateString);
	const now = new Date();
	const diff = now.getTime() - date.getTime();

	const minute = 60 * 1000;
	const hour = 60 * minute;
	const day = 24 * hour;

	if (diff < minute) return 'Just now';
	if (diff < hour) return `${Math.floor(diff / minute)} minutes ago`;
	if (diff < day) return `${Math.floor(diff / hour)} hours ago`;
	if (diff < 7 * day) return `${Math.floor(diff / day)} days ago`;

	return date.toLocaleDateString('en-US', {
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		hour: 'numeric',
		minute: '2-digit'
	});
}

export function getMimeTypeIcon(mimeType: string): string {
	const normalized = mimeType.toLowerCase();
	if (normalized.startsWith('image/')) return '🖼️';
	if (normalized.startsWith('video/')) return '🎥';
	if (normalized.startsWith('audio/')) return '🎵';
	if (normalized.includes('pdf')) return '📄';
	if (
		normalized.includes('zip') ||
		normalized.includes('tar') ||
		normalized.includes('gzip') ||
		normalized.includes('archive')
	) {
		return '📦';
	}
	if (normalized.includes('msword') || normalized.includes('wordprocessingml')) {
		return '📝';
	}
	if (normalized.includes('ms-excel') || normalized.includes('spreadsheetml')) {
		return '📊';
	}
	if (normalized.includes('ms-powerpoint') || normalized.includes('presentationml')) {
		return '📽️';
	}
	if (normalized.startsWith('text/')) return '📃';
	return '📄';
}
