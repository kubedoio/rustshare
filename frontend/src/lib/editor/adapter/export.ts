/**
 * Rich Markdown Editor — Export Utilities
 *
 * Handles file downloads and print triggering.
 */

/**
 * Triggers a browser download of a text string as a file.
 */
export function downloadTextFile(
	filename: string,
	content: string,
	mimeType: string = 'text/markdown'
): void {
	const blob = new Blob([content], { type: mimeType });
	const url = URL.createObjectURL(blob);
	const link = document.createElement('a');

	link.href = url;
	link.download = filename;
	document.body.appendChild(link);
	link.click();

	// Cleanup
	document.body.removeChild(link);
	URL.revokeObjectURL(url);
}

/**
 * Triggers the browser print dialog.
 */
export function triggerPrint(): void {
	window.print();
}

/**
 * Formats a filename for export by removing invalid characters.
 */
export function formatExportFilename(title: string, extension: string): string {
	const sanitized = title
		.toLowerCase()
		.replace(/[^a-z0-9]/g, '-')
		.replace(/-+/g, '-')
		.replace(/^-|-$/g, '');

	return `${sanitized || 'document'}.${extension}`;
}
