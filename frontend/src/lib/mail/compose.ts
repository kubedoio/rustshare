import DOMPurify from 'isomorphic-dompurify';

export type MailBody = { type: 'empty' | 'html' | 'text'; content: string };

export function mailBodyText(body: MailBody): string {
	let text = body.content;
	if (body.type === 'html') {
		const sanitized = DOMPurify.sanitize(body.content, {
			ALLOWED_TAGS: ['br', 'p', 'div', 'li', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6'],
			ALLOWED_ATTR: []
		}) as string;
		const parsed = new DOMParser().parseFromString(sanitized, 'text/html');
		text = extractMailText(parsed.body)
			.replace(/\u00a0/g, ' ')
			.trim();
	}
	// Tiptap parses initial content as HTML; serialize through a text node so literal
	// markup remains visible text and cannot become an editor node or load resources.
	const container = document.createElement('div');
	container.textContent = text;
	return container.innerHTML;
}

function extractMailText(node: Node): string {
	let text = '';
	for (const child of node.childNodes) {
		if (child.nodeType === Node.TEXT_NODE) {
			text += child.textContent ?? '';
		} else if (child.nodeName === 'BR') {
			text += '\n';
		} else {
			text += extractMailText(child);
			if (['P', 'DIV', 'LI', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6'].includes(child.nodeName)) {
				text += '\n';
			}
		}
	}
	return text;
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
