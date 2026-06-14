<!--
  RichMarkdownViewer — read-only Markdown renderer using Tiptap for
  consistent rendering with the editor.
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { createEventDispatcher } from 'svelte';
	import { markdownToHtml } from '../adapter/markdown';
	import { sanitizeHtml } from '../adapter/security';
	import { resolveAttachmentPaths } from '../adapter/attachments';
	import type { RichMarkdownAttachment } from '../types';

	let {
		content = '',
		attachments = [],
		vaultId = undefined
	}: {
		content?: string;
		attachments?: RichMarkdownAttachment[];
		vaultId?: string;
	} = $props();

	let renderedHtml = $state('');
	let parseError = $state<string | null>(null);

	const dispatch = createEventDispatcher<{
		open: { attachment: RichMarkdownAttachment };
	}>();

	function findAttachmentByUrl(url: string): RichMarkdownAttachment | undefined {
		// Match /api/v1/files/{id}/preview or /api/v1/files/{id}/content
		const match = url.match(/\/api\/v1\/files\/([^/]+)\/(?:preview|content)/);
		if (match) {
			return attachments.find((a) => a.id === match[1]);
		}
		// Also try matching by relative path (fallback for unresolved paths)
		return attachments.find((a) => url.includes(a.path) || url.endsWith(a.filename));
	}

	function escapeHtml(str: string): string {
		return str
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/"/g, '&quot;')
			.replace(/'/g, '&#39;');
	}

	function resolveWikilinkHtml(html: string, vaultId: string | undefined): string {
		if (!vaultId) {
			// Without vaultId: show wikilink text without link, image placeholders as text
			return html
				.replace(
					/<a[^>]*\sdata-wikilink="([^"]*)"[^>]*>([^<]*)<\/a>/g,
					'<span class="wikilink-text">$2</span>'
				)
				.replace(/<img[^>]*>/g, (tag) => {
					if (!tag.includes('data-wikilink-src=')) return tag;
					const altMatch = tag.match(/alt="([^"]*)"/);
					const srcMatch = tag.match(/data-wikilink-src="([^"]*)"/);
					const label = altMatch ? altMatch[1] : srcMatch ? srcMatch[1] : '';
					return `<span class="wikilink-missing">[image: ${label}]</span>`;
				});
		}

		// With vaultId: resolve image sources to vault file API URLs
		return html.replace(/<img[^>]*>/g, (tag) => {
			if (!tag.includes('data-wikilink-src=')) return tag;
			const srcMatch = tag.match(/data-wikilink-src="([^"]*)"/);
			const altMatch = tag.match(/alt="([^"]*)"/);
			const path = srcMatch ? srcMatch[1] : '';
			const alt = altMatch ? altMatch[1] : path;
			const apiUrl = `/api/vault-sync/v1/vaults/${escapeHtml(vaultId)}/files/${encodeURIComponent(path)}`;
			return `<img src="${apiUrl}" alt="${escapeHtml(alt)}" />`;
		});
	}

	function handleViewerClick(event: MouseEvent) {
		const target = event.target as HTMLElement;

		// Check for <a> tag clicks
		const anchor = target.closest('a') as HTMLAnchorElement | null;
		if (anchor) {
			const wikilink = anchor.getAttribute('data-wikilink');
			if (wikilink && vaultId) {
				event.preventDefault();
				event.stopPropagation();
				goto(`/vaults/${vaultId}?preview=${encodeURIComponent(wikilink)}`);
				return;
			}

			const href = anchor.getAttribute('href');
			if (!href) return;

			const attachment = findAttachmentByUrl(href);
			if (attachment) {
				event.preventDefault();
				event.stopPropagation();
				dispatch('open', { attachment });
				return;
			}
			// External or unknown link: open in new tab
			event.preventDefault();
			window.open(href, '_blank');
			return;
		}

		// Check for <img> tag clicks
		const img = target.closest('img') as HTMLImageElement | null;
		if (img) {
			const wikilinkSrc = img.getAttribute('data-wikilink-src');
			if (wikilinkSrc && vaultId) {
				event.preventDefault();
				event.stopPropagation();
				goto(`/vaults/${vaultId}?preview=${encodeURIComponent(wikilinkSrc)}`);
				return;
			}

			const src = img.getAttribute('src');
			if (!src) return;

			const attachment = findAttachmentByUrl(src);
			if (attachment) {
				event.preventDefault();
				event.stopPropagation();
				dispatch('open', { attachment });
			}
			return;
		}
	}

	$effect(() => {
		const resolvedContent = attachments?.length
			? resolveAttachmentPaths(content, attachments)
			: content;
		const result = markdownToHtml(resolvedContent);
		if (result.success) {
			renderedHtml = sanitizeHtml(resolveWikilinkHtml(result.html, vaultId));
			parseError = null;
		} else {
			parseError = result.error || 'Failed to render Markdown';
			renderedHtml = '';
		}
	});
</script>

<div class="rich-markdown-viewer">
	{#if parseError}
		<div class="viewer-warning">
			<p class="warning-text">⚠ Could not render this document.</p>
			<p class="warning-detail">{parseError}</p>
		</div>
		<!-- Raw Markdown fallback -->
		<pre class="viewer-raw-fallback">{content}</pre>
	{:else if renderedHtml}
		<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
		<div class="viewer-content prose max-w-none" onclick={handleViewerClick}>
			{@html renderedHtml}
		</div>
	{:else}
		<div class="viewer-empty">
			<p>No content</p>
		</div>
	{/if}
</div>

<style>
	.rich-markdown-viewer {
		padding: 1.5rem;
		color: var(--color-base-content, #374151);
		line-height: 1.7;
	}

	.viewer-warning {
		padding: 0.75rem 1rem;
		background: color-mix(in oklab, var(--color-warning, #f59e0b) 10%, transparent);
		border: 1px solid color-mix(in oklab, var(--color-warning, #f59e0b) 25%, transparent);
		border-radius: 0.5rem;
		margin-bottom: 1rem;
	}

	.warning-text {
		font-weight: 600;
		margin: 0;
	}

	.warning-detail {
		font-size: 0.875rem;
		margin: 0.25rem 0 0;
		opacity: 0.8;
	}

	.viewer-raw-fallback {
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.875rem;
		line-height: 1.6;
		padding: 1rem;
		background: var(--color-base-200, #f3f4f6);
		border-radius: 0.5rem;
		overflow-x: auto;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.viewer-empty {
		text-align: center;
		padding: 2rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.6;
		font-style: italic;
	}

	/* Prose styling for rendered content */
	.viewer-content :global(h1) {
		font-size: 1.875rem;
		font-weight: 700;
		margin: 1.5rem 0 0.75rem;
		line-height: 1.2;
	}

	.viewer-content :global(h2) {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 1.25rem 0 0.625rem;
		line-height: 1.3;
	}

	.viewer-content :global(h3) {
		font-size: 1.25rem;
		font-weight: 600;
		margin: 1rem 0 0.5rem;
		line-height: 1.4;
	}

	.viewer-content :global(p) {
		margin: 0 0 0.75rem;
	}

	.viewer-content :global(ul) {
		margin: 0 0 0.75rem;
		padding-left: 1.5rem;
		list-style-type: disc;
	}

	.viewer-content :global(ol) {
		margin: 0 0 0.75rem;
		padding-left: 1.5rem;
		list-style-type: decimal;
	}

	.viewer-content :global(li) {
		margin-bottom: 0.25rem;
	}

	/* Task lists */
	.viewer-content :global(ul.editor-task-list) {
		list-style: none;
		padding-left: 0;
		margin: 0 0 0.75rem;
	}

	.viewer-content :global(li.editor-task-item) {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		margin-bottom: 0.25rem;
	}

	.viewer-content :global(.editor-task-item label) {
		display: flex;
		align-items: center;
		margin-top: 0.15rem;
	}

	.viewer-content :global(.editor-task-item input[type='checkbox']) {
		width: 1rem;
		height: 1rem;
		accent-color: var(--color-primary, #3b82f6);
	}

	.viewer-content :global(.editor-task-item[data-checked='true'] > p),
	.viewer-content :global(.editor-task-item[data-checked='true'] > div > p) {
		text-decoration: line-through;
		opacity: 0.6;
	}

	.viewer-content :global(blockquote) {
		border-left: 3px solid var(--color-primary, #3b82f6);
		padding-left: 1rem;
		margin: 0.75rem 0;
		color: var(--color-base-content, #6b7280);
		font-style: italic;
	}

	.viewer-content :global(pre) {
		background: var(--color-base-200, #1e1e1e);
		padding: 1rem;
		border-radius: 0.5rem;
		overflow-x: auto;
		margin: 0.75rem 0;
		font-size: 0.875rem;
	}

	.viewer-content :global(code) {
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.875em;
	}

	.viewer-content :global(:not(pre) > code) {
		background: var(--color-base-300, #e5e7eb);
		padding: 0.125rem 0.375rem;
		border-radius: 0.25rem;
	}

	.viewer-content :global(pre code) {
		background: none;
		padding: 0;
		font-size: 0.875rem;
	}

	.viewer-content :global(u) {
		text-decoration: underline;
	}

	.viewer-content :global(a) {
		color: var(--color-primary, #3b82f6);
		text-decoration: underline;
	}

	.viewer-content :global(hr) {
		border: none;
		border-top: 1px solid var(--color-base-300, #e5e7eb);
		margin: 1.5rem 0;
	}

	/* Images */
	.viewer-content :global(img) {
		max-width: 100%;
		border-radius: 0.5rem;
		margin: 0.75rem 0;
		height: auto;
		cursor: pointer;
	}

	/* Tables */
	.viewer-content :global(table) {
		border-collapse: collapse;
		width: 100%;
		margin: 0.75rem 0;
		table-layout: fixed;
	}

	.viewer-content :global(td),
	.viewer-content :global(th) {
		border: 1px solid var(--color-base-300, #d1d5db);
		padding: 0.5rem 0.75rem;
		vertical-align: top;
		min-width: 80px;
	}

	.viewer-content :global(th) {
		background: var(--color-base-200, #f3f4f6);
		font-weight: 600;
		text-align: left;
	}

	/* Wikilink placeholders */
	.viewer-content :global(.wikilink-text) {
		color: var(--color-base-content, #374151);
	}

	.viewer-content :global(.wikilink-missing) {
		color: var(--color-base-content, #9ca3af);
		font-style: italic;
		opacity: 0.7;
	}
</style>
