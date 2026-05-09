import DOMPurify from 'isomorphic-dompurify';

/**
 * Basic markdown to HTML renderer
 */
export function renderMarkdown(markdown: string): string {
	if (!markdown) return '';

	// 1. Escape HTML
	let rawHtml = markdown
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;');

	// 2. Preserve code blocks with placeholders so their content
	//    is not affected by list/bold/italic processing
	const codeBlocks: string[] = [];
	rawHtml = rawHtml.replace(/```(\w+)?\n([\s\S]*?)```/g, (_, lang, code) => {
		const placeholder = `<span data-code-block="${codeBlocks.length}"></span>`;
		codeBlocks.push(
			`<pre class="bg-base-300 p-3 rounded-lg overflow-x-auto my-2"><code>${code}</code></pre>`
		);
		return placeholder;
	});

	// 3. Unordered lists — match consecutive lines starting with "- "
	rawHtml = rawHtml.replace(
		/(^|\n)([ \t]*- .+(?:\n[ \t]*- .+)*)/g,
		(_, prefix, listBlock) => {
			const items = listBlock
				.split('\n')
				.map((line: string) => {
					const content = line.replace(/^[ \t]*- /, '');
					return `<li>${content}</li>`;
				})
				.join('');
			return prefix + `<ul class="list-disc my-2 pl-5">${items}</ul>`;
		}
	);

	// 4. Ordered lists — match consecutive lines starting with "digits. "
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

	// 5. Blockquotes
	rawHtml = rawHtml.replace(
		/^&gt; (.*$)/gim,
		'<blockquote class="border-l-4 border-primary pl-4 my-2 italic">$1</blockquote>'
	);

	// 6. Headers
	rawHtml = rawHtml
		.replace(/^### (.*$)/gim, '<h3>$1</h3>')
		.replace(/^## (.*$)/gim, '<h2>$1</h2>')
		.replace(/^# (.*$)/gim, '<h1>$1</h1>');

	// 7. Bold (must come before italic to avoid conflicts with ***)
	rawHtml = rawHtml
		.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
		.replace(/__(.*?)__/g, '<strong>$1</strong>');

	// 8. Italic
	rawHtml = rawHtml
		.replace(/\*(.*?)\*/g, '<em>$1</em>')
		.replace(/_(.*?)_/g, '<em>$1</em>');

	// 9. Inline code
	rawHtml = rawHtml.replace(
		/`([^`]+)`/g,
		'<code class="bg-base-300 px-1 rounded text-sm">$1</code>'
	);

	// 10. Links
	rawHtml = rawHtml.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, text: string, url: string) => {
		const safeUrl = url.replace(/^javascript:/i, '#');
		return `<a href="${safeUrl}" class="text-primary hover:underline" target="_blank" rel="noopener noreferrer">${text}</a>`;
	});

	// 11. Images
	rawHtml = rawHtml.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt: string, url: string) => {
		const safeUrl = url.replace(/^javascript:/i, '#');
		return `<img src="${safeUrl}" alt="${alt}" class="max-w-full rounded-lg my-2" />`;
	});

	// 12. Horizontal rule
	rawHtml = rawHtml.replace(/^---$/gim, '<hr class="my-4 border-base-300" />');

	// 13. Line breaks
	rawHtml = rawHtml.replace(/\n/g, '<br />');

	// 14. Restore code blocks
	codeBlocks.forEach((block, i) => {
		rawHtml = rawHtml.replace(`<span data-code-block="${i}"></span>`, block);
	});

	return DOMPurify.sanitize(rawHtml, { ADD_ATTR: ['target'] });
}
