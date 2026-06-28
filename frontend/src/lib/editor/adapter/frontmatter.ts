/**
 * Frontmatter helpers for OKF-native Markdown documents.
 *
 * Keeps YAML frontmatter opaque to the editor: the UI only edits the body,
 * while unknown frontmatter fields are preserved across save cycles.
 */

export interface SplitFrontmatterResult {
	frontmatter: string;
	body: string;
	hasFrontmatter: boolean;
}

const FRONTMATTER_DELIMITER = '---';

/**
 * Splits Markdown content into a YAML frontmatter block and the body.
 *
 * A frontmatter block is recognized only when it starts at the very
 * beginning of the document with `---` followed by a newline and is closed
 * by a matching `---` line. The closing delimiter is kept inside the
 * `frontmatter` value so `wrapFrontmatter` can round-trip it exactly.
 */
export function splitFrontmatter(content: string): SplitFrontmatterResult {
	if (!content.startsWith(`${FRONTMATTER_DELIMITER}\n`)) {
		return { frontmatter: '', body: content, hasFrontmatter: false };
	}

	const closeDelimiter = `\n${FRONTMATTER_DELIMITER}\n`;
	const closeIndex = content.indexOf(closeDelimiter, FRONTMATTER_DELIMITER.length + 1);
	if (closeIndex === -1) {
		// Malformed: opening delimiter without a proper close line.
		return { frontmatter: '', body: content, hasFrontmatter: false };
	}

	const frontmatterEnd = closeIndex + closeDelimiter.length;
	const frontmatter = content.slice(0, frontmatterEnd);
	const body = content.slice(frontmatterEnd);

	return { frontmatter, body, hasFrontmatter: true };
}

/**
 * Recombines a frontmatter block and body back into canonical Markdown.
 *
 * The helper is idempotent: it trims the body before joining so saves do not
 * accumulate blank lines between the delimiter and the content.
 */
export function wrapFrontmatter(frontmatter: string, body: string): string {
	const normalizedFrontmatter = frontmatter.endsWith('\n') ? frontmatter : `${frontmatter}\n`;
	const normalizedBody = body.replace(/^\n+/, '');
	return `${normalizedFrontmatter}${normalizedBody}`;
}

/**
 * Extracts the first level-1 heading from a Markdown body.
 *
 * Ignores leading whitespace and other non-H1 lines. Returns `null` when no
 * H1 is present.
 */
export function extractH1(body: string): string | null {
	const lines = body.split('\n');
	for (const line of lines) {
		const trimmed = line.trim();
		if (trimmed === '') continue;

		const match = trimmed.match(/^#\s+(.*)$/);
		if (match) {
			return match[1].trim();
		}
	}
	return null;
}
