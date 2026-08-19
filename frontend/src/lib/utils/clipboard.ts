/**
 * Copy text to the system clipboard. Failures are swallowed; the caller is
 * expected to provide its own feedback if needed.
 */
export function copyToClipboard(text: string): void {
	navigator.clipboard?.writeText(text)?.catch(() => {});
}
