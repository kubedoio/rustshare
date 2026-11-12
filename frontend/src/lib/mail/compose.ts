import DOMPurify from 'isomorphic-dompurify';

export type MailBody = { type: 'empty' | 'html' | 'text'; content: string };

export function mailBodyText(body: MailBody): string {
	let text = body.content;
	if (body.type === 'html') {
		text = text
			.replace(/<br\s*\/?>/gi, '\n')
			.replace(/<\/(p|div|li|h[1-6])>/gi, '\n')
			.replace(/<[^>]+>/g, '')
			.replace(/&nbsp;/g, ' ')
			.replace(/&amp;/g, '&')
			.trim();
	}
	// Tiptap parses initial content as HTML; strip markup again at this boundary.
	return DOMPurify.sanitize(text, { ALLOWED_TAGS: [], ALLOWED_ATTR: [], KEEP_CONTENT: true });
}

export function quoteMailBody(text: string): string {
	return text
		.split(/\r?\n/)
		.map((line) => `> ${line}`)
		.join('\n');
}

export function uniqueMailAddresses(addresses: string[], excluded: string[] = []): string[] {
	const seen = new Set(excluded.map((addr) => addr.trim().toLowerCase()).filter(Boolean));
	const result: string[] = [];
	for (const address of addresses) {
		const trimmed = address.trim();
		const key = trimmed.toLowerCase();
		if (!trimmed || seen.has(key)) continue;
		seen.add(key);
		result.push(trimmed);
	}
	return result;
}
