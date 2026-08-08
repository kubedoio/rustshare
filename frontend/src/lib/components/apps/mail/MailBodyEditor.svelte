<!--
  MailBodyEditor — lightweight rich-text body editor for mail compose.
  Uses the same Tiptap extension set as the note editor (same Markdown
  dialect), but with a compact email-appropriate toolbar.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Editor } from '@tiptap/core';
	import { getEditorExtensions } from '$lib/editor/adapter/extensions';
	import { editorToMarkdown } from '$lib/editor/adapter/markdown';
	import {
		Bold,
		Italic,
		Underline,
		Strikethrough,
		Code,
		Link2,
		Link2Off,
		List,
		ListOrdered,
		TextQuote,
		Undo2,
		Redo2
	} from 'lucide-svelte';

	let {
		content = '',
		placeholder = 'Message',
		onChange
	}: {
		content?: string;
		placeholder?: string;
		onChange: (markdown: string) => void;
	} = $props();

	let element: HTMLDivElement;
	let editor: Editor | null = $state.raw(null);
	// Bumped on editor transactions so toolbar active states re-render.
	let stateVersion = $state(0);

	onMount(() => {
		editor = new Editor({
			element,
			extensions: getEditorExtensions({ placeholder }),
			content,
			onUpdate: ({ editor: e }) => onChange(editorToMarkdown(e)),
			onTransaction: () => (stateVersion += 1)
		});
		return () => editor?.destroy();
	});

	// Sync externally replaced content (e.g. opening a different draft) without
	// clobbering in-progress typing and without echoing back through onChange.
	let lastExternalContent = $state(content);
	$effect(() => {
		if (!editor || content === lastExternalContent) return;
		lastExternalContent = content;
		if (editorToMarkdown(editor) !== content) {
			editor.commands.setContent(content, { emitUpdate: false });
		}
	});

	function isActive(name: string, attrs?: Record<string, unknown>): boolean {
		void stateVersion;
		return editor?.isActive(name, attrs) ?? false;
	}

	function canRun(name: 'undo' | 'redo'): boolean {
		void stateVersion;
		if (!editor) return false;
		return name === 'undo' ? editor.can().undo() : editor.can().redo();
	}

	function run(action: () => void) {
		action();
		stateVersion += 1;
	}

	function toggleLink() {
		if (!editor) return;
		if (editor.isActive('link')) {
			run(() => editor?.chain().focus().unsetLink().run());
			return;
		}
		const previous = editor.getAttributes('link').href as string | undefined;
		const url = window.prompt('Link URL', previous ?? 'https://');
		if (!url) return;
		run(() => editor?.chain().focus().extendMarkRange('link').setLink({ href: url }).run());
	}

	const buttonBase = 'btn btn-xs btn-ghost btn-square';
	const buttonActive = 'bg-base-300 text-base-content';
</script>

<div
	class="mail-body-editor rounded-md border border-[var(--rs-border)] bg-[var(--rs-surface-raised)] focus-within:border-brand-500/60"
>
	<div
		class="flex flex-wrap items-center gap-0.5 border-b border-[var(--rs-border)] px-2 py-1"
		role="toolbar"
		aria-label="Formatting"
	>
		<button
			type="button"
			class="{buttonBase} {isActive('bold') ? buttonActive : ''}"
			title="Bold"
			aria-label="Bold"
			aria-pressed={isActive('bold')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleBold().run())}
		>
			<Bold size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('italic') ? buttonActive : ''}"
			title="Italic"
			aria-label="Italic"
			aria-pressed={isActive('italic')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleItalic().run())}
		>
			<Italic size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('underline') ? buttonActive : ''}"
			title="Underline"
			aria-label="Underline"
			aria-pressed={isActive('underline')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleUnderline().run())}
		>
			<Underline size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('strike') ? buttonActive : ''}"
			title="Strikethrough"
			aria-label="Strikethrough"
			aria-pressed={isActive('strike')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleStrike().run())}
		>
			<Strikethrough size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('code') ? buttonActive : ''}"
			title="Inline code"
			aria-label="Inline code"
			aria-pressed={isActive('code')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleCode().run())}
		>
			<Code size={13} />
		</button>
		<div class="mx-1 h-4 w-px bg-[var(--rs-border)]" aria-hidden="true"></div>
		<button
			type="button"
			class="{buttonBase} {isActive('link') ? buttonActive : ''}"
			title="Add or edit link"
			aria-label="Add or edit link"
			onmousedown={(e) => e.preventDefault()}
			onclick={toggleLink}
		>
			<Link2 size={13} />
		</button>
		<button
			type="button"
			class={buttonBase}
			title="Remove link"
			aria-label="Remove link"
			disabled={!isActive('link')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().unsetLink().run())}
		>
			<Link2Off size={13} />
		</button>
		<div class="mx-1 h-4 w-px bg-[var(--rs-border)]" aria-hidden="true"></div>
		<button
			type="button"
			class="{buttonBase} {isActive('bulletList') ? buttonActive : ''}"
			title="Bullet list"
			aria-label="Bullet list"
			aria-pressed={isActive('bulletList')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleBulletList().run())}
		>
			<List size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('orderedList') ? buttonActive : ''}"
			title="Numbered list"
			aria-label="Numbered list"
			aria-pressed={isActive('orderedList')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleOrderedList().run())}
		>
			<ListOrdered size={13} />
		</button>
		<button
			type="button"
			class="{buttonBase} {isActive('blockquote') ? buttonActive : ''}"
			title="Quote"
			aria-label="Quote"
			aria-pressed={isActive('blockquote')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().toggleBlockquote().run())}
		>
			<TextQuote size={13} />
		</button>
		<div class="mx-1 h-4 w-px bg-[var(--rs-border)]" aria-hidden="true"></div>
		<button
			type="button"
			class={buttonBase}
			title="Undo"
			aria-label="Undo"
			disabled={!canRun('undo')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().undo().run())}
		>
			<Undo2 size={13} />
		</button>
		<button
			type="button"
			class={buttonBase}
			title="Redo"
			aria-label="Redo"
			disabled={!canRun('redo')}
			onmousedown={(e) => e.preventDefault()}
			onclick={() => run(() => editor?.chain().focus().redo().run())}
		>
			<Redo2 size={13} />
		</button>
	</div>
	<div
		bind:this={element}
		class="mail-body-editor-content"
		role="textbox"
		aria-multiline="true"
		aria-label={placeholder}
	></div>
</div>

<style>
	.mail-body-editor-content :global(.ProseMirror) {
		padding: 0.625rem 0.875rem;
		outline: none;
		min-height: 12rem;
		max-height: 22rem;
		overflow-y: auto;
		font-size: 0.875rem;
		line-height: 1.55;
		color: var(--color-base-content, #374151);
	}

	.mail-body-editor-content :global(.ProseMirror p.is-editor-empty:first-child::before) {
		content: attr(data-placeholder);
		float: left;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.45;
		pointer-events: none;
		height: 0;
	}

	.mail-body-editor-content :global(.ProseMirror p) {
		margin: 0 0 0.5rem;
	}

	.mail-body-editor-content :global(.ProseMirror ul:not(.editor-task-list)) {
		list-style-type: disc;
		padding-left: 1.25rem;
		margin: 0 0 0.5rem;
	}

	.mail-body-editor-content :global(.ProseMirror ol) {
		list-style-type: decimal;
		padding-left: 1.25rem;
		margin: 0 0 0.5rem;
	}

	.mail-body-editor-content :global(.ProseMirror blockquote) {
		border-left: 3px solid var(--rs-border, #ded6ca);
		padding-left: 0.75rem;
		margin: 0 0 0.5rem;
		color: var(--rs-text-muted, #6c665f);
	}

	.mail-body-editor-content :global(.ProseMirror code) {
		background: var(--rs-surface-muted, #efe8de);
		border-radius: 0.25rem;
		padding: 0.05rem 0.3rem;
		font-size: 0.8125rem;
	}

	.mail-body-editor-content :global(.ProseMirror pre) {
		background: var(--rs-surface-muted, #efe8de);
		border-radius: 0.375rem;
		padding: 0.625rem 0.75rem;
		margin: 0 0 0.5rem;
		font-size: 0.8125rem;
	}

	.mail-body-editor-content :global(.ProseMirror a) {
		color: var(--color-primary, #c65a1e);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.mail-body-editor-content :global(.ProseMirror hr) {
		border: none;
		border-top: 1px solid var(--rs-border, #ded6ca);
		margin: 0.75rem 0;
	}
</style>
