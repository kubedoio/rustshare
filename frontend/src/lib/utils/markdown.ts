import DOMPurify from 'isomorphic-dompurify';

/**
 * Basic markdown to HTML renderer
 */
export function renderMarkdown(markdown: string): string {
	if (!markdown) return '';

	const rawHtml = markdown
			// Escape HTML
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			// Headers
			.replace(/^### (.*$)/gim, '<h3>$1</h3>')
			.replace(/^## (.*$)/gim, '<h2>$1</h2>')
			.replace(/^# (.*$)/gim, '<h1>$1</h1>')
			// Bold
			.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
			.replace(/__(.*?)__/g, '<strong>$1</strong>')
			// Italic
			.replace(/\*(.*?)\*/g, '<em>$1</em>')
			.replace(/_(.*?)_/g, '<em>$1</em>')
			// Code inline
			.replace(/`([^`]+)`/g, '<code class="bg-base-300 px-1 rounded text-sm">$1</code>')
			// Code blocks
			.replace(/```(\w+)?\n([\s\S]*?)```/g, '<pre class="bg-base-300 p-3 rounded-lg overflow-x-auto my-2"><code>$2</code></pre>')
			// Blockquotes
			.replace(/^&gt; (.*$)/gim, '<blockquote class="border-l-4 border-primary pl-4 my-2 italic">$1</blockquote>')
			// Links
			.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_, text: string, url: string) => {
				const safeUrl = url.replace(/^javascript:/i, '#');
				return `<a href="${safeUrl}" class="text-primary hover:underline" target="_blank" rel="noopener noreferrer">${text}</a>`;
			})
			// Images
			.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt: string, url: string) => {
				const safeUrl = url.replace(/^javascript:/i, '#');
				return `<img src="${safeUrl}" alt="${alt}" class="max-w-full rounded-lg my-2" />`;
			})
			// Unordered lists
			.replace(/^\s*- (.*$)/gim, '<li class="ml-4">$1</li>')
			.replace(/(<li.*<\/li>\n)+/g, '<ul class="list-disc my-2">$&</ul>')
			// Ordered lists
			.replace(/^\s*\d+\. (.*$)/gim, '<li class="ml-4">$1</li>')
			.replace(/(<li.*<\/li>\n)+/g, '<ol class="list-decimal my-2">$&</ol>')
			// Horizontal rule
			.replace(/^---$/gim, '<hr class="my-4 border-base-300" />')
			// Line breaks
			.replace(/\n/g, '<br />');

	return DOMPurify.sanitize(rawHtml, { ADD_ATTR: ['target'] });
}
