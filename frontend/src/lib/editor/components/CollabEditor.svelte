<!--
  CollabEditor — note editor with queued markdown autosave.
  Wraps RichMarkdownEditor and persists edits without requiring the save button.
-->
<script lang="ts">
	import { onDestroy, createEventDispatcher } from 'svelte';
	import type { Editor } from '@tiptap/core';
	import RichMarkdownEditor from './RichMarkdownEditor.svelte';
	import type { EditorPermissions, RichMarkdownAttachment } from '../types';
	import { WRITE_PERMISSIONS } from '../types';

	const dispatch = createEventDispatcher<{
		change: { markdown: string };
		save: { content: string };
	}>();

	/** Note ID retained for compatibility with existing document page wiring */
	export let docId: string;

	/** Initial Markdown content (loaded from server) */
	export let content: string = '';

	/** User permissions */
	export let permissions: EditorPermissions = WRITE_PERMISSIONS;

	/** Whether editing is enabled */
	export let editable: boolean = true;

	/** Whether an attachment handler is available */
	export let hasAttachmentHandler: boolean = false;

	/** Expose current Markdown for parent reads */
	export let currentMarkdown: string = content;

	let editorComponent: RichMarkdownEditor;
	let status: 'saved' | 'unsaved' | 'saving' | 'error' = 'saved';
	let autosaveTimer: ReturnType<typeof setTimeout> | null = null;
	let lastSavedMarkdown = content;
	let pendingMarkdown: string | null = null;
	let inFlightMarkdown: string | null = null;
	let lastError: string | null = null;

	$: resolvedEditable = permissions.canEdit && editable;

	onDestroy(() => {
		if (autosaveTimer) {
			clearTimeout(autosaveTimer);
			autosaveTimer = null;
		}
		flushPendingSave();
	});

	function startSave(markdown: string): void {
		inFlightMarkdown = markdown;
		pendingMarkdown = null;
		status = 'saving';
		lastError = null;
		dispatch('save', { content: markdown });
	}

	function flushPendingSave(): void {
		const markdown =
			pendingMarkdown ?? editorComponent?.getMarkdown() ?? currentMarkdown ?? content;

		if (inFlightMarkdown) {
			pendingMarkdown = markdown;
			status = 'saving';
			return;
		}

		if (markdown === lastSavedMarkdown) {
			pendingMarkdown = null;
			status = 'saved';
			return;
		}

		startSave(markdown);
	}

	function handleEditorChange(event: CustomEvent<{ markdown: string }>) {
		currentMarkdown = event.detail.markdown;
		pendingMarkdown = currentMarkdown;
		if (!inFlightMarkdown) {
			status = currentMarkdown === lastSavedMarkdown ? 'saved' : 'unsaved';
		}
		dispatch('change', { markdown: currentMarkdown });
		if (autosaveTimer) clearTimeout(autosaveTimer);
		autosaveTimer = setTimeout(() => {
			autosaveTimer = null;
			flushPendingSave();
		}, 1000);
	}

	function getStatusLabel() {
		switch (status) {
			case 'unsaved':
				return 'Unsaved';
			case 'saving':
				return 'Saving...';
			case 'saved':
				return 'Saved';
			case 'error':
				return lastError ?? 'Autosave error';
		}
	}

	function getStatusDotClass() {
		switch (status) {
			case 'saved':
				return 'status-dot-synced';
			case 'unsaved':
				return 'status-dot-unsaved';
			case 'saving':
				return 'status-dot-saving';
			case 'error':
				return 'status-dot-disconnected';
		}
	}

	export function getMarkdown(): string {
		if (!editorComponent) return content;
		const markdown = editorComponent.getMarkdown();
		currentMarkdown = markdown;
		pendingMarkdown = markdown;
		return markdown;
	}

	export function getEditor(): Editor | null {
		return editorComponent?.getEditor() || null;
	}

	export function setContent(markdown: string): void {
		editorComponent?.setContent(markdown);
	}

	export function markSaved(markdown?: string): void {
		const savedMarkdown = markdown ?? inFlightMarkdown;
		if (savedMarkdown != null) {
			lastSavedMarkdown = savedMarkdown;
		}
		inFlightMarkdown = null;
		lastError = null;

		const latestMarkdown = pendingMarkdown ?? currentMarkdown;
		if (latestMarkdown !== lastSavedMarkdown) {
			pendingMarkdown = latestMarkdown;
			status = 'unsaved';
			if (autosaveTimer) clearTimeout(autosaveTimer);
			autosaveTimer = setTimeout(() => {
				autosaveTimer = null;
				flushPendingSave();
			}, 0);
			return;
		}

		pendingMarkdown = null;
		status = 'saved';
	}

	export function markSaveError(message = 'Autosave failed'): void {
		pendingMarkdown = pendingMarkdown ?? inFlightMarkdown ?? currentMarkdown;
		inFlightMarkdown = null;
		lastError = message;
		status = 'error';
	}

	export function insertAttachment(attachment: RichMarkdownAttachment) {
		// Delegate to RichMarkdownEditor if needed
	}
</script>

<div class="collab-editor" data-doc-id={docId}>
	<div class="collab-status-bar">
		<span class="status-indicator">
			<span class="status-dot {getStatusDotClass()}"></span>
			<span class="status-text">{getStatusLabel()}</span>
		</span>
	</div>
	<div class="editor-surface">
		<RichMarkdownEditor
			bind:this={editorComponent}
			{content}
			editable={resolvedEditable}
			{hasAttachmentHandler}
			bind:currentMarkdown
			on:change={handleEditorChange}
			on:ready
			on:attachment
			on:sketch
			on:filedrop
			on:paste
		/>
	</div>
</div>

<style>
	.collab-editor {
		display: flex;
		flex-direction: column;
		height: 100%;
		border: 1px solid var(--color-base-300, #e5e7eb);
		border-radius: 0.5rem;
		overflow: hidden;
		background: var(--color-base-100, #fff);
	}

	.collab-status-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 0.75rem;
		border-bottom: 1px solid var(--color-base-300, #e5e7eb);
		background: var(--color-base-200, #f9fafb);
		font-size: 0.75rem;
		color: var(--color-base-content, #6b7280);
	}

	.status-indicator {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.status-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
	}

	.status-dot-unsaved {
		background: #f59e0b;
	}

	.status-dot-saving {
		background: #3b82f6;
		animation: pulse 1.5s infinite;
	}

	.status-dot-synced {
		background: #22c55e;
	}

	.status-dot-disconnected {
		background: #ef4444;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.4;
		}
	}

	.editor-surface {
		flex: 1;
		overflow: hidden;
		min-height: 0;
	}
</style>
