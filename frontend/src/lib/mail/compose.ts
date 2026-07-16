export type MailBody = { type: 'empty' | 'html' | 'text'; content: string };

export function mailBodyText(body: MailBody): string {
	if (body.type !== 'html') return body.content;
	return body.content
		.replace(/<br\s*\/?>/gi, '\n')
		.replace(/<\/(p|div|li|h[1-6])>/gi, '\n')
		.replace(/<[^>]+>/g, '')
		.replace(/&nbsp;/g, ' ')
		.replace(/&amp;/g, '&')
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.trim();
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
