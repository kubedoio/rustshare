<!--
  RichMarkdownViewer — read-only Markdown renderer using Tiptap for
  consistent rendering with the editor.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { markdownToHtml } from '../adapter/markdown';
	import { sanitizeHtml } from '../adapter/security';

	/** Markdown content to render */
	export let content: string = '';

	let renderedHtml = '';
	let parseError: string | null = null;

	$: {
		const result = markdownToHtml(content);
		if (result.success) {
			renderedHtml = sanitizeHtml(result.html);
			parseError = null;
		} else {
			parseError = result.error || 'Failed to render Markdown';
			renderedHtml = '';
		}
	}
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
		<div class="viewer-content prose max-w-none">
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
</style>
