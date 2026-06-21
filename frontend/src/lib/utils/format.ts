export function formatFileSize(bytes: number): string {
	if (!Number.isFinite(bytes) || bytes < 0) return '0 Bytes';
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

/**
 * Get a user-friendly file type label from MIME type and filename
 * Falls back to file extension for unknown/binary types
 */
export function getFileTypeLabel(mimeType: string, fileName: string): string {
	const normalized = mimeType.toLowerCase();
	const extension = fileName.split('.').pop()?.toLowerCase() || '';

	// Image types
	if (normalized.startsWith('image/')) {
		if (normalized.includes('png')) return 'PNG';
		if (normalized.includes('jpeg') || normalized.includes('jpg')) return 'JPEG';
		if (normalized.includes('gif')) return 'GIF';
		if (normalized.includes('svg')) return 'SVG';
		if (normalized.includes('webp')) return 'WebP';
		if (normalized.includes('bmp')) return 'BMP';
		if (normalized.includes('tiff')) return 'TIFF';
		return 'Image';
	}

	// Video types
	if (normalized.startsWith('video/')) {
		if (normalized.includes('mp4')) return 'MP4';
		if (normalized.includes('webm')) return 'WebM';
		if (normalized.includes('avi')) return 'AVI';
		if (normalized.includes('mov') || normalized.includes('quicktime')) return 'QuickTime';
		if (normalized.includes('mkv')) return 'MKV';
		return 'Video';
	}

	// Audio types
	if (normalized.startsWith('audio/')) {
		if (normalized.includes('mpeg') || normalized.includes('mp3')) return 'MP3';
		if (normalized.includes('wav')) return 'WAV';
		if (normalized.includes('ogg')) return 'OGG';
		if (normalized.includes('flac')) return 'FLAC';
		if (normalized.includes('aac')) return 'AAC';
		if (normalized.includes('m4a')) return 'M4A';
		return 'Audio';
	}

	// Document types
	if (normalized.includes('pdf')) return 'PDF';
	if (
		normalized.includes('msword') ||
		normalized.includes('wordprocessingml') ||
		extension === 'doc' ||
		extension === 'docx'
	) {
		return 'Word';
	}
	if (
		normalized.includes('ms-excel') ||
		normalized.includes('spreadsheetml') ||
		extension === 'xls' ||
		extension === 'xlsx'
	) {
		return 'Excel';
	}
	if (
		normalized.includes('ms-powerpoint') ||
		normalized.includes('presentationml') ||
		extension === 'ppt' ||
		extension === 'pptx'
	) {
		return 'PowerPoint';
	}
	if (normalized.includes('openxmlformats-officedocument')) {
		if (extension === 'docx') return 'Word';
		if (extension === 'xlsx') return 'Excel';
		if (extension === 'pptx') return 'PowerPoint';
	}

	// Text types
	if (normalized.startsWith('text/')) {
		if (normalized.includes('html')) return 'HTML';
		if (normalized.includes('css')) return 'CSS';
		if (normalized.includes('javascript')) return 'JavaScript';
		if (normalized.includes('json')) return 'JSON';
		if (normalized.includes('xml')) return 'XML';
		if (normalized.includes('markdown') || extension === 'md') return 'Markdown';
		if (normalized.includes('plain')) return 'Text';
		return 'Text';
	}

	// Archive types
	if (
		normalized.includes('zip') ||
		normalized.includes('tar') ||
		normalized.includes('gzip') ||
		normalized.includes('archive') ||
		normalized.includes('compressed')
	) {
		if (extension === 'zip') return 'ZIP';
		if (extension === 'tar') return 'TAR';
		if (extension === 'gz' || extension === 'tgz') return 'GZIP';
		if (extension === 'rar') return 'RAR';
		if (extension === '7z') return '7Z';
		return 'Archive';
	}

	// Code types
	if (normalized.includes('javascript') || extension === 'js' || extension === 'jsx')
		return 'JavaScript';
	if (normalized.includes('typescript') || extension === 'ts' || extension === 'tsx')
		return 'TypeScript';
	if (normalized.includes('json')) return 'JSON';
	if (normalized.includes('xml')) return 'XML';
	if (normalized.includes('sql')) return 'SQL';

	// Fallback to extension for binary/unknown types
	if (normalized.includes('octet-stream') || normalized.includes('binary')) {
		if (extension) return extension.toUpperCase();
	}

	// Try to get subtype as last resort
	const subtype = mimeType.split('/')[1];
	if (subtype && subtype !== 'octet-stream') {
		return subtype.toUpperCase();
	}

	// Final fallback to extension
	if (extension) return extension.toUpperCase();

	return 'File';
}

/**
 * Check if a file is an MS Office document
 */
export function isOfficeFile(mimeType: string, fileName: string): boolean {
	if (!mimeType || !fileName) return false;
	const normalized = mimeType.toLowerCase();
	const name = fileName.toLowerCase();

	const officeMimeTypes = [
		'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
		'application/msword',
		'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
		'application/vnd.ms-excel',
		'application/vnd.openxmlformats-officedocument.presentationml.presentation',
		'application/vnd.ms-powerpoint'
	];

	const officeExtensions = ['.docx', '.doc', '.xlsx', '.xls', '.pptx', '.ppt'];

	if (officeMimeTypes.some((m) => normalized.includes(m))) return true;
	if (officeExtensions.some((ext) => name.endsWith(ext))) return true;

	return false;
}

/**
 * Get Office file type label
 */
export function getOfficeFileType(
	mimeType: string,
	fileName: string
): 'word' | 'excel' | 'powerpoint' | null {
	if (!mimeType || !fileName) return null;
	const normalized = mimeType.toLowerCase();
	const name = fileName.toLowerCase();

	if (
		normalized.includes('wordprocessingml') ||
		normalized.includes('msword') ||
		name.endsWith('.docx') ||
		name.endsWith('.doc')
	) {
		return 'word';
	}
	if (
		normalized.includes('spreadsheetml') ||
		normalized.includes('ms-excel') ||
		name.endsWith('.xlsx') ||
		name.endsWith('.xls')
	) {
		return 'excel';
	}
	if (
		normalized.includes('presentationml') ||
		normalized.includes('ms-powerpoint') ||
		name.endsWith('.pptx') ||
		name.endsWith('.ppt')
	) {
		return 'powerpoint';
	}

	return null;
}

/**
 * Truncate a filename in the middle, preserving the start and the extension
 * e.g., "verylongfilenametoupload.png" -> "very..load.png"
 */
export function truncateFilename(filename: string, maxLength: number = 20): string {
	if (!filename || filename.length <= maxLength) return filename;

	const parts = filename.split('.');
	const hasExtension = parts.length > 1;

	if (!hasExtension) {
		// No extension, just truncate the end
		return filename.substring(0, maxLength - 2) + '..';
	}

	const extension = parts.pop()!;
	const name = parts.join('.');

	// If the name is very short and the extension is extremely long,
	// just truncate the whole string
	if (name.length <= 4) {
		return filename.substring(0, maxLength - 2) + '..';
	}

	// Calculate how much space we have for the name
	// Account for the ".." separator and the extension+dot
	const charsForName = maxLength - 2 - (extension.length + 1);

	if (charsForName <= 4) {
		// Not enough space for middle truncation, just do standard start truncation
		return filename.substring(0, Math.max(3, maxLength - 2)) + '..';
	}

	const charsForStart = Math.ceil(charsForName / 2);
	const charsForEnd = Math.floor(charsForName / 2);

	const start = name.substring(0, charsForStart);
	const end = name.substring(name.length - charsForEnd);

	return `${start}..${end}.${extension}`;
}

export function formatDistanceToNow(
	dateInput: Date | string,
	options?: { addSuffix?: boolean }
): string {
	const date = dateInput instanceof Date ? dateInput : new Date(dateInput);
	const now = new Date();
	const diffMs = now.getTime() - date.getTime();
	const isFuture = diffMs < 0;
	const absMs = Math.abs(diffMs);

	const diffSec = Math.floor(absMs / 1000);
	const diffMin = Math.floor(diffSec / 60);
	const diffHour = Math.floor(diffMin / 60);
	const diffDay = Math.floor(diffHour / 24);
	const diffMonth = Math.floor(diffDay / 30);
	const diffYear = Math.floor(diffMonth / 12);

	let prefix = '';
	let suffix = '';
	if (options?.addSuffix) {
		if (isFuture) {
			prefix = 'in ';
		} else {
			suffix = ' ago';
		}
	}

	let distance = '';
	if (diffSec < 60) {
		distance = 'less than a minute';
	} else if (diffMin < 60) {
		distance = diffMin === 1 ? '1 minute' : `${diffMin} minutes`;
	} else if (diffHour < 24) {
		distance = diffHour === 1 ? 'about 1 hour' : `about ${diffHour} hours`;
	} else if (diffDay < 30) {
		distance = diffDay === 1 ? '1 day' : `${diffDay} days`;
	} else if (diffMonth < 12) {
		distance = diffMonth === 1 ? 'about 1 month' : `about ${diffMonth} months`;
	} else {
		distance = diffYear === 1 ? 'about 1 year' : `about ${diffYear} years`;
	}

	return `${prefix}${distance}${suffix}`;
}
