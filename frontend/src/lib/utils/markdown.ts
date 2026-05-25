import DOMPurify from 'isomorphic-dompurify';

/**
 * Basic markdown to HTML renderer
 */
export function renderMarkdown(markdown: string): string {
	if (!markdown) return '';

	// 1. Escape HTML
	let rawHtml = markdown.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

	// 2. Preserve code blocks with placeholders so their content
	//    is not affected by list/bold/italic/table processing
	const codeBlocks: string[] = [];
	rawHtml = rawHtml.replace(/```(\w+)?\n([\s\S]*?)```/g, (_, lang, code) => {
		const placeholder = `<span data-code-block="${codeBlocks.length}"></span>`;
		codeBlocks.push(
			`<pre class="bg-base-300 p-3 rounded-lg overflow-x-auto my-2"><code>${code}</code></pre>`
		);
		return placeholder;
	});

	// 3. Tables (GFM-style) — process before lists to avoid conflicts
	rawHtml = processMarkdownTables(rawHtml);

	// 4. Task lists — match consecutive lines starting with "- [ ] " or "- [x] "
	rawHtml = rawHtml.replace(
		/(^|\n)([ \t]*- \[[ xX]\] .+(?:\n[ \t]*- \[[ xX]\] .+)*)/g,
		(_, prefix, listBlock) => {
			const items = listBlock
				.split('\n')
				.map((line: string) => {
					const match = line.match(/^[ \t]*- \[([ xX])\] (.*)$/);
					if (!match) return '';
					const checked = match[1].toLowerCase() === 'x' ? 'checked' : '';
					const content = match[2];
					return `<li class="flex items-start gap-2 mb-1"><input type="checkbox" disabled ${checked} class="mt-1 w-4 h-4 accent-primary shrink-0" />${content}</li>`;
				})
				.join('');
			return prefix + `<ul class="list-none my-2 pl-0">${items}</ul>`;
		}
	);

	// 5. Unordered lists — match consecutive lines starting with "- "
	rawHtml = rawHtml.replace(/(^|\n)([ \t]*- .+(?:\n[ \t]*- .+)*)/g, (_, prefix, listBlock) => {
		const items = listBlock
			.split('\n')
			.map((line: string) => {
				const content = line.replace(/^[ \t]*- /, '');
				return `<li>${content}</li>`;
			})
			.join('');
		return prefix + `<ul class="list-disc my-2 pl-5">${items}</ul>`;
	});

	// 6. Ordered lists — match consecutive lines starting with "digits. "
	rawHtml = rawHtml.replace(
		/(^|\n)([ \t]*\d+\. .+(?:\n[ \t]*\d+\. .+)*)/g,
		(_, prefix, listBlock) => {
			const items = listBlock
				.split('\n')
				.map((line: string) => {
					const content = line.replace(/^[ \t]*\d+\. /, '');
					return `<li>${content}</li>`;
				})
				.join('');
			return prefix + `<ol class="list-decimal my-2 pl-5">${items}</ol>`;
		}
	);

	// 7. Blockquotes
	rawHtml = rawHtml.replace(
		/^&gt; (.*$)/gim,
		'<blockquote class="border-l-4 border-primary pl-4 my-2 italic">$1</blockquote>'
	);

	// 8. Headers
	rawHtml = rawHtml
		.replace(/^### (.*$)/gim, '<h3>$1</h3>')
		.replace(/^## (.*$)/gim, '<h2>$1</h2>')
		.replace(/^# (.*$)/gim, '<h1>$1</h1>');

	// 9. Bold (must come before italic to avoid conflicts with ***)
	rawHtml = rawHtml
		.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
		.replace(/__(.*?)__/g, '<strong>$1</strong>');

	// 10. Italic
	rawHtml = rawHtml.replace(/\*(.*?)\*/g, '<em>$1</em>').replace(/_(.*?)_/g, '<em>$1</em>');

	// 11. Inline code
	rawHtml = rawHtml.replace(
		/`([^`]+)`/g,
		'<code class="bg-base-300 px-1 rounded text-sm">$1</code>'
	);

	// 12. Links
	rawHtml = rawHtml.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, text: string, url: string) => {
		const safeUrl = url.replace(/^javascript:/i, '#');
		return `<a href="${safeUrl}" class="text-primary hover:underline" target="_blank" rel="noopener noreferrer">${text}</a>`;
	});

	// 13. Images
	rawHtml = rawHtml.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt: string, url: string) => {
		const safeUrl = url.replace(/^javascript:/i, '#');
		return `<img src="${safeUrl}" alt="${alt}" class="max-w-full rounded-lg my-2" />`;
	});

	// 14. Horizontal rule
	rawHtml = rawHtml.replace(/^---$/gim, '<hr class="my-4 border-base-300" />');

	// 15. Line breaks
	rawHtml = rawHtml.replace(/\n/g, '<br />');

	// 16. Restore code blocks
	codeBlocks.forEach((block, i) => {
		rawHtml = rawHtml.replace(`<span data-code-block="${i}"></span>`, block);
	});

	return DOMPurify.sanitize(rawHtml, { ADD_ATTR: ['target'] });
}

/**
 * Parse GFM-style Markdown tables into HTML.
 * Operates on HTML-escaped text (before inline formatting is applied).
 */
function processMarkdownTables(text: string): string {
	const lines = text.split('\n');
	const result: string[] = [];
	let i = 0;

	while (i < lines.length) {
		const line = lines[i];
		// A table row must contain at least one |
		if (line.includes('|')) {
			// Gather consecutive lines that contain |
			const tableLines: string[] = [];
			let j = i;
			while (j < lines.length && lines[j].includes('|')) {
				tableLines.push(lines[j]);
				j++;
			}

			// Need at least 2 lines and second line must be a separator
			if (tableLines.length >= 2 && isTableSeparator(tableLines[1])) {
				result.push(convertTableLinesToHtml(tableLines));
				i = j;
				continue;
			}
		}

		result.push(lines[i]);
		i++;
	}

	return result.join('\n');
}

function isTableSeparator(line: string): boolean {
	const trimmed = line.trim();
	return /^[\s|:-]+$/.test(trimmed) && trimmed.length > 0 && /-/.test(trimmed);
}

function parseTableRow(line: string): string[] {
	const trimmed = line.trim();
	let content = trimmed.startsWith('|') ? trimmed.slice(1) : trimmed;
	if (content.endsWith('|')) {
		content = content.slice(0, -1);
	}
	return content.split('|');
}

function convertTableLinesToHtml(lines: string[]): string {
	const headerCells = parseTableRow(lines[0]);
	const bodyRows: string[][] = [];
	for (let i = 2; i < lines.length; i++) {
		bodyRows.push(parseTableRow(lines[i]));
	}

	let html = '<table style="border-collapse:collapse;width:100%;margin:0.75rem 0;">';

	html += '<thead><tr>';
	headerCells.forEach((cell) => {
		html += `<th style="border:1px solid #d1d5db;padding:0.5rem 0.75rem;background:#f3f4f6;font-weight:600;text-align:left;">${cell.trim()}</th>`;
	});
	html += '</tr></thead>';

	if (bodyRows.length > 0) {
		html += '<tbody>';
		bodyRows.forEach((cells) => {
			html += '<tr>';
			cells.forEach((cell) => {
				html += `<td style="border:1px solid #d1d5db;padding:0.5rem 0.75rem;">${cell.trim()}</td>`;
			});
			html += '</tr>';
		});
		html += '</tbody>';
	}

	html += '</table>';
	return html;
}
