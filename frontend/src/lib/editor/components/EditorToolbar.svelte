<!--
  EditorToolbar — formatting toolbar for the Rich Markdown Editor.
  Groups: Headings | Text formatting | Lists | Blocks | Insert | More
-->
<script lang="ts">
	import type { Editor } from '@tiptap/core';
	import {
		Heading1,
		Heading2,
		Heading3,
		Bold,
		Italic,
		Underline,
		List,
		ListOrdered,
		ListChecks,
		Quote,
		Code,
		Braces,
		Link,
		Unlink,
		Minus,
		Table,
		Paperclip,
		Undo2,
		Redo2
	} from 'lucide-svelte';
	import { createEventDispatcher } from 'svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';

	let {
		editor = null,
		hasAttachmentHandler = false
	}: { editor?: Editor | null; hasAttachmentHandler?: boolean } = $props();

	const dispatch = createEventDispatcher<{
		attachment: void;
		more: void;
	}>();

	// Force reactivity on selection/transaction changes
	let _tick = $state(0);
	$effect(() => {
		if (editor) {
			const updateTick = () => (_tick = _tick + 1);
			editor.on('selectionUpdate', updateTick);
			editor.on('transaction', updateTick);
			return () => {
				editor.off('selectionUpdate', updateTick);
				editor.off('transaction', updateTick);
			};
		}
	});

	function is(name: string, attrs?: Record<string, unknown>): boolean {
		if (!editor || _tick < 0) return false;
		return editor.isActive(name, attrs);
	}

	function canRun(command: 'undo' | 'redo'): boolean {
		if (!editor || _tick < 0) return false;
		try {
			const checker = editor.can() as unknown as Record<string, () => boolean>;
			return checker[command]?.() ?? false;
		} catch {
			return false;
		}
	}

	function cmd(cb: (e: Editor) => void) {
		return () => {
			if (!editor) return;
			cb(editor);
		};
	}

	let showLinkPrompt = $state(false);
	let linkUrl = $state('');

	function toggleLink() {
		if (!editor) return;
		if (editor.isActive('link')) {
			editor.chain().focus().unsetLink().run();
			return;
		}
		linkUrl = '';
		showLinkPrompt = true;
	}

	function handleLinkConfirm(href: string) {
		showLinkPrompt = false;
		if (href && editor) {
			editor.chain().focus().setLink({ href }).run();
		}
	}

	function insertTable() {
		if (!editor) return;
		editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run();
	}
</script>

{#if editor}
	<div class="editor-toolbar" role="toolbar" aria-label="Formatting toolbar">
		<!-- History -->
		<div class="toolbar-group">
			<button
				type="button"
				class="toolbar-btn"
				onclick={cmd((e) => e.chain().focus().undo().run())}
				disabled={!canRun('undo')}
				title="Undo"
				aria-label="Undo"
			>
				<Undo2 size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				onclick={cmd((e) => e.chain().focus().redo().run())}
				disabled={!canRun('redo')}
				title="Redo"
				aria-label="Redo"
			>
				<Redo2 size={16} />
			</button>
		</div>

		<div class="toolbar-divider"></div>

		<!-- Headings -->
		<div class="toolbar-group">
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('heading', { level: 1 })}
				onclick={cmd((e) => e.chain().focus().toggleHeading({ level: 1 }).run())}
				title="Heading 1"
				aria-label="Heading 1"
			>
				<Heading1 size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('heading', { level: 2 })}
				onclick={cmd((e) => e.chain().focus().toggleHeading({ level: 2 }).run())}
				title="Heading 2"
				aria-label="Heading 2"
			>
				<Heading2 size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('heading', { level: 3 })}
				onclick={cmd((e) => e.chain().focus().toggleHeading({ level: 3 }).run())}
				title="Heading 3"
				aria-label="Heading 3"
			>
				<Heading3 size={16} />
			</button>
		</div>

		<div class="toolbar-divider"></div>

		<!-- Text formatting -->
		<div class="toolbar-group">
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('bold')}
				onclick={cmd((e) => e.chain().focus().toggleBold().run())}
				title="Bold (⌘B)"
				aria-label="Bold"
			>
				<Bold size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('italic')}
				onclick={cmd((e) => e.chain().focus().toggleItalic().run())}
				title="Italic (⌘I)"
				aria-label="Italic"
			>
				<Italic size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('underline')}
				onclick={cmd((e) => e.chain().focus().toggleUnderline().run())}
				title="Underline (⌘U)"
				aria-label="Underline"
			>
				<Underline size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('code')}
				onclick={cmd((e) => e.chain().focus().toggleCode().run())}
				title="Inline code"
				aria-label="Inline code"
			>
				<Code size={16} />
			</button>
		</div>

		<div class="toolbar-divider"></div>

		<!-- Lists -->
		<div class="toolbar-group">
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('bulletList')}
				onclick={cmd((e) => e.chain().focus().toggleBulletList().run())}
				title="Bullet list"
				aria-label="Bullet list"
			>
				<List size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('orderedList')}
				onclick={cmd((e) => e.chain().focus().toggleOrderedList().run())}
				title="Numbered list"
				aria-label="Numbered list"
			>
				<ListOrdered size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('taskList')}
				onclick={cmd((e) => e.chain().focus().toggleTaskList().run())}
				title="Task list"
				aria-label="Task list"
			>
				<ListChecks size={16} />
			</button>
		</div>

		<div class="toolbar-divider"></div>

		<!-- Block elements -->
		<div class="toolbar-group">
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('blockquote')}
				onclick={cmd((e) => e.chain().focus().toggleBlockquote().run())}
				title="Blockquote"
				aria-label="Blockquote"
			>
				<Quote size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('codeBlock')}
				onclick={cmd((e) => e.chain().focus().toggleCodeBlock().run())}
				title="Code block"
				aria-label="Code block"
			>
				<Braces size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				class:active={is('link')}
				onclick={toggleLink}
				title={is('link') ? 'Remove link' : 'Insert link'}
				aria-label={is('link') ? 'Remove link' : 'Insert link'}
			>
				{#if is('link')}
					<Unlink size={16} />
				{:else}
					<Link size={16} />
				{/if}
			</button>
			<button
				type="button"
				class="toolbar-btn"
				onclick={insertTable}
				title="Insert table"
				aria-label="Insert table"
			>
				<Table size={16} />
			</button>
			<button
				type="button"
				class="toolbar-btn"
				onclick={cmd((e) => e.chain().focus().setHorizontalRule().run())}
				title="Horizontal rule"
				aria-label="Horizontal rule"
			>
				<Minus size={16} />
			</button>
		</div>

		<!-- Attachment (only if handler available) -->
		{#if hasAttachmentHandler}
			<div class="toolbar-divider"></div>
			<div class="toolbar-group">
				<button
					type="button"
					class="toolbar-btn"
					onclick={() => dispatch('attachment')}
					title="Attach file"
					aria-label="Attach file"
				>
					<Paperclip size={16} />
				</button>
			</div>
		{/if}
	</div>
{/if}

<PromptModal
	open={showLinkPrompt}
	title="Insert Link"
	message="Enter URL:"
	defaultValue={linkUrl}
	confirmLabel="Insert"
	onConfirm={handleLinkConfirm}
	onCancel={() => (showLinkPrompt = false)}
/>

<style>
	.editor-toolbar {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-base-300, #e5e7eb);
		background: var(--color-base-200, #f3f4f6);
		flex-wrap: wrap;
	}

	.toolbar-group {
		display: flex;
		align-items: center;
		gap: 0.125rem;
	}

	.toolbar-divider {
		width: 1px;
		height: 1.25rem;
		background: var(--color-base-300, #d1d5db);
		margin: 0 0.25rem;
	}

	.toolbar-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.375rem;
		border-radius: 0.375rem;
		border: none;
		background: transparent;
		color: var(--color-base-content, #374151);
		cursor: pointer;
		transition:
			background 0.15s,
			color 0.15s;
	}

	.toolbar-btn:hover {
		background: var(--color-base-300, #d1d5db);
	}

	.toolbar-btn:disabled {
		cursor: not-allowed;
		opacity: 0.4;
	}

	.toolbar-btn:disabled:hover {
		background: transparent;
	}

	.toolbar-btn.active {
		background: var(--color-primary, #3b82f6);
		color: var(--color-primary-content, #fff);
	}

	.toolbar-btn:focus-visible {
		outline: 2px solid var(--color-primary, #3b82f6);
		outline-offset: 1px;
	}
</style>
