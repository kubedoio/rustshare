import { describe, it, expect } from 'vitest';
import {
	formatFileSize,
	formatDate,
	formatAbsoluteDate,
	formatAbsoluteDateTime,
	getMimeTypeIcon,
	formatDistanceToNow
} from '$lib/utils/format';

describe('format utilities', () => {
	describe('formatFileSize', () => {
		it('should format bytes', () => {
			expect(formatFileSize(0)).toBe('0 B');
			expect(formatFileSize(512)).toBe('512 byte');
			expect(formatFileSize(1023)).toBe('1,023 byte');
		});

		it('should format kilobytes', () => {
			expect(formatFileSize(1024)).toBe('1 kB');
			expect(formatFileSize(1536)).toBe('1.5 kB');
			expect(formatFileSize(10240)).toBe('10 kB');
		});

		it('should format megabytes', () => {
			expect(formatFileSize(1048576)).toBe('1 MB');
			expect(formatFileSize(1572864)).toBe('1.5 MB');
			expect(formatFileSize(10485760)).toBe('10 MB');
		});

		it('should format gigabytes', () => {
			expect(formatFileSize(1073741824)).toBe('1 GB');
			expect(formatFileSize(1610612736)).toBe('1.5 GB');
			expect(formatFileSize(10737418240)).toBe('10 GB');
		});

		it('should format terabytes', () => {
			expect(formatFileSize(1099511627776)).toBe('1 TB');
			expect(formatFileSize(1649267441664)).toBe('1.5 TB');
		});

		it('should round to 2 decimal places', () => {
			expect(formatFileSize(1536)).toBe('1.5 kB');
			expect(formatFileSize(1638)).toBe('1.6 kB'); // 1.599609375 KB
		});
	});

	describe('formatDate', () => {
		it('should format ISO date strings', () => {
			const date = '2024-03-19T10:30:00Z';
			const formatted = formatDate(date);

			// Check that it contains expected parts
			expect(formatted).toMatch(/Mar/);
			expect(formatted).toMatch(/19/);
			expect(formatted).toMatch(/2024/);
			expect(formatted).toMatch(/\d{1,2}:\d{2}/); // Time portion
		});

		it('should handle different date formats', () => {
			const dates = ['2024-01-01T00:00:00Z', '2024-12-31T23:59:59Z', '2024-06-15T12:00:00Z'];

			dates.forEach((date) => {
				const formatted = formatDate(date);
				expect(formatted).toBeTruthy();
				expect(typeof formatted).toBe('string');
			});
		});

		it('should be consistent for same date', () => {
			const date = '2024-03-19T10:30:00Z';
			const formatted1 = formatDate(date);
			const formatted2 = formatDate(date);
			expect(formatted1).toBe(formatted2);
		});
	});

	describe('formatAbsoluteDateTime', () => {
		it('formats date and time in the canonical detail format', () => {
			const formatted = formatAbsoluteDateTime('2026-07-29T20:01:00Z');

			expect(formatted).toMatch(/Jul/);
			expect(formatted).toMatch(/29/);
			expect(formatted).toMatch(/2026/);
			expect(formatted).toMatch(/\d{1,2}:\d{2}/); // time portion
		});

		it('accepts Date instances', () => {
			const date = new Date('2026-01-05T09:30:00Z');
			expect(formatAbsoluteDateTime(date)).toBe(formatAbsoluteDateTime(date.toISOString()));
		});

		it('is deterministic for the same input', () => {
			const input = '2026-07-29T20:01:00Z';
			expect(formatAbsoluteDateTime(input)).toBe(formatAbsoluteDateTime(input));
		});
	});

	describe('formatAbsoluteDate', () => {
		it('formats date only in the canonical detail format', () => {
			const formatted = formatAbsoluteDate('2026-07-29T20:01:00Z');

			expect(formatted).toMatch(/Jul/);
			expect(formatted).toMatch(/29/);
			expect(formatted).toMatch(/2026/);
			expect(formatted).not.toMatch(/\d{1,2}:\d{2}/); // no time portion
		});

		it('accepts Date instances', () => {
			const date = new Date('2026-01-05T09:30:00Z');
			expect(formatAbsoluteDate(date)).toBe(formatAbsoluteDate(date.toISOString()));
		});
	});

	describe('getMimeTypeIcon', () => {
		it('should return image icon for image types', () => {
			expect(getMimeTypeIcon('image/png')).toBe('🖼️');
			expect(getMimeTypeIcon('image/jpeg')).toBe('🖼️');
			expect(getMimeTypeIcon('image/gif')).toBe('🖼️');
			expect(getMimeTypeIcon('image/svg+xml')).toBe('🖼️');
		});

		it('should return video icon for video types', () => {
			expect(getMimeTypeIcon('video/mp4')).toBe('🎥');
			expect(getMimeTypeIcon('video/mpeg')).toBe('🎥');
			expect(getMimeTypeIcon('video/quicktime')).toBe('🎥');
		});

		it('should return audio icon for audio types', () => {
			expect(getMimeTypeIcon('audio/mpeg')).toBe('🎵');
			expect(getMimeTypeIcon('audio/wav')).toBe('🎵');
			expect(getMimeTypeIcon('audio/ogg')).toBe('🎵');
		});

		it('should return PDF icon for PDF', () => {
			expect(getMimeTypeIcon('application/pdf')).toBe('📄');
		});

		it('should return archive icon for compressed files', () => {
			expect(getMimeTypeIcon('application/zip')).toBe('📦');
			expect(getMimeTypeIcon('application/x-tar')).toBe('📦');
			expect(getMimeTypeIcon('application/gzip')).toBe('📦');
		});

		it('should return document icon for document types', () => {
			expect(getMimeTypeIcon('application/msword')).toBe('📝');
			expect(
				getMimeTypeIcon('application/vnd.openxmlformats-officedocument.wordprocessingml.document')
			).toBe('📝');
		});

		it('should return spreadsheet icon for spreadsheet types', () => {
			expect(getMimeTypeIcon('application/vnd.ms-excel')).toBe('📊');
			expect(
				getMimeTypeIcon('application/vnd.openxmlformats-officedocument.spreadsheetml.sheet')
			).toBe('📊');
		});

		it('should return presentation icon for presentation types', () => {
			expect(getMimeTypeIcon('application/vnd.ms-powerpoint')).toBe('📽️');
			expect(
				getMimeTypeIcon('application/vnd.openxmlformats-officedocument.presentationml.presentation')
			).toBe('📽️');
		});

		it('should return text icon for text types', () => {
			expect(getMimeTypeIcon('text/plain')).toBe('📃');
			expect(getMimeTypeIcon('text/html')).toBe('📃');
			expect(getMimeTypeIcon('text/css')).toBe('📃');
		});

		it('should return generic icon for unknown types', () => {
			expect(getMimeTypeIcon('application/octet-stream')).toBe('📄');
			expect(getMimeTypeIcon('unknown/type')).toBe('📄');
			expect(getMimeTypeIcon('')).toBe('📄');
		});

		it('should be case insensitive', () => {
			expect(getMimeTypeIcon('IMAGE/PNG')).toBe('🖼️');
			expect(getMimeTypeIcon('Video/MP4')).toBe('🎥');
			expect(getMimeTypeIcon('APPLICATION/PDF')).toBe('📄');
		});
	});

	describe('formatDistanceToNow', () => {
		it('should format past dates', () => {
			const now = new Date();

			const thirtySecsAgo = new Date(now.getTime() - 30 * 1000);
			expect(formatDistanceToNow(thirtySecsAgo)).toBe('30 seconds ago');
			expect(formatDistanceToNow(thirtySecsAgo, { addSuffix: true })).toBe('30 seconds ago');

			const fiveMinsAgo = new Date(now.getTime() - 5 * 60 * 1000);
			expect(formatDistanceToNow(fiveMinsAgo)).toBe('5 minutes ago');
			expect(formatDistanceToNow(fiveMinsAgo, { addSuffix: true })).toBe('5 minutes ago');

			const threeHoursAgo = new Date(now.getTime() - 3 * 60 * 60 * 1000);
			expect(formatDistanceToNow(threeHoursAgo)).toBe('3 hours ago');
			expect(formatDistanceToNow(threeHoursAgo, { addSuffix: true })).toBe('3 hours ago');
		});

		it('should format future dates', () => {
			const now = new Date();

			const thirtySecsInFuture = new Date(now.getTime() + 30 * 1000);
			expect(formatDistanceToNow(thirtySecsInFuture)).toBe('in 30 seconds');
			expect(formatDistanceToNow(thirtySecsInFuture, { addSuffix: true })).toBe('in 30 seconds');

			const fiveMinsInFuture = new Date(now.getTime() + 5 * 60 * 1000);
			expect(formatDistanceToNow(fiveMinsInFuture)).toBe('in 5 minutes');
			expect(formatDistanceToNow(fiveMinsInFuture, { addSuffix: true })).toBe('in 5 minutes');

			const threeHoursInFuture = new Date(now.getTime() + 3 * 60 * 60 * 1000);
			expect(formatDistanceToNow(threeHoursInFuture)).toBe('in 3 hours');
			expect(formatDistanceToNow(threeHoursInFuture, { addSuffix: true })).toBe('in 3 hours');
		});
	});
});
