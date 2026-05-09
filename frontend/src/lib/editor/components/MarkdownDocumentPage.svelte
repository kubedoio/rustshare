<!--
  MarkdownDocumentPage — full page wrapper that combines viewer/editor
  with title, mode toggle, save status, and action bar.
  Reusable across Notes, Decisions, Meetings and file browser.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy } from 'svelte';
	import {
		ArrowLeft,
		Eye,
		Pencil,
		Save,
		Check,
		AlertCircle,
		Download,
		FileText,
		MoreHorizontal,
		Paperclip,
		ChevronRight
	} from 'lucide-svelte';
	import type {
		EditorMode,
		EditorSaveStatus,
		EditorPermissions,
		RichMarkdownAttachment
	} from '../types';
	import { WRITE_PERMISSIONS, READ_ONLY_PERMISSIONS } from '../types';
	import RichMarkdownEditor from './RichMarkdownEditor.svelte';
	import RichMarkdownViewer from './RichMarkdownViewer.svelte';
	import AttachmentPanel from './AttachmentPanel.svelte';
	import PrintableDocumentView from './PrintableDocumentView.svelte';
	import { insertAttachmentIntoEditor } from '../adapter/attachments';
	import { downloadTextFile, formatExportFilename, triggerPrint } from '../adapter/export';

	/** Purposeful Colors from CSS */
	const PURPOSEFUL_COLORS = [
		{ name: 'Default', value: null, class: 'bg-base-300' },
		{ name: 'Blue', value: 'blue', class: 'bg-[var(--rs-accent-blue)]' },
		{ name: 'Green', value: 'green', class: 'bg-[var(--rs-accent-green)]' },
		{ name: 'Red', value: 'red', class: 'bg-[var(--rs-accent-red)]' },
		{ name: 'Orange', value: 'orange', class: 'bg-[var(--rs-accent-orange)]' },
		{ name: 'Purple', value: 'purple', class: 'bg-[var(--rs-accent-purple)]' },
		{ name: 'Pink', value: 'pink', class: 'bg-[var(--rs-accent-pink)]' }
	];

	/** Note title */
	export let title: string = '';

	/** Initial Markdown content */
	export let content: string = '';

	/** Current note color */
	export let color: string | null = null;

	/** Current editor mode */
	export let mode: EditorMode = 'read';

	/** User permissions */
	export let permissions: EditorPermissions = READ_ONLY_PERMISSIONS;

	/** Save status */
	export let saveStatus: EditorSaveStatus = 'saved';

	/** Document revision/version for conflict handling */
	export let revision: number | string | undefined = undefined;

	/** Optional module/path label (legacy, use breadcrumb instead) */
	export let label: string = '';

	/** Breadcrumb trail: [{ label, onClick? }, ...] */
	export let breadcrumb: Array<{ label: string; onClick?: () => void }> = [];

	/** Optional metadata line (e.g. last modified date) */
	export let metadata: string = '';

	/** Attachment list */
	export let attachments: RichMarkdownAttachment[] = [];

	/** Autosave delay in ms (0 to disable) */
	export let autosaveDelay: number = 1500;

	/** Whether to show the back button */
	export let showBack: boolean = true;

	const dispatch = createEventDispatcher<{
		save: { content: string; revision?: number | string; color?: string | null };
		modechange: { mode: EditorMode };
		back: void;
		export: { format: 'markdown' | 'print' };
		upload: { files: File[] };
		sketch: { blob: Blob; filename: string };
		delete: { attachment: RichMarkdownAttachment };
	}>();

	let editorComponent: RichMarkdownEditor;
	let currentMarkdown: string = content;
	let isAttachmentsOpen = false;
	let autosaveTimer: ReturnType<typeof setTimeout> | null = null;

	$: canEdit = permissions.canEdit;
	$: isEditing = mode === 'edit' && canEdit;

	// Cleanup timer on unmount
	onDestroy(() => {
		if (autosaveTimer) clearTimeout(autosaveTimer);
	});

	function toggleMode() {
		const newMode: EditorMode = mode === 'read' ? 'edit' : 'read';

		// If switching from edit to read, ensure any pending autosave is flushed
		if (mode === 'edit' && saveStatus === 'unsaved') {
			handleSave();
		}

		mode = newMode;
		dispatch('modechange', { mode: newMode });
	}

	function handleSave() {
		if (autosaveTimer) {
			clearTimeout(autosaveTimer);
			autosaveTimer = null;
		}

		if (!editorComponent || saveStatus === 'saving') return;

		const md = editorComponent.getMarkdown();
		saveStatus = 'saving';
		dispatch('save', { content: md, revision });
	}

	function handleEditorChange(event: CustomEvent<{ markdown: string }>) {
		currentMarkdown = event.detail.markdown;

		if (saveStatus !== 'unsaved') {
			saveStatus = 'unsaved';
		}

		// Trigger autosave
		if (autosaveDelay > 0) {
			if (autosaveTimer) clearTimeout(autosaveTimer);
			autosaveTimer = setTimeout(() => {
				handleSave();
			}, autosaveDelay);
		}
	}

	function handleBack() {
		dispatch('back');
	}

	function handleExportMarkdown() {
		const filename = formatExportFilename(title, 'md');
		downloadTextFile(filename, currentMarkdown || content);
		dispatch('export', { format: 'markdown' });
	}

	function handlePrint() {
		// For a simple implementation, we just trigger browser print on the current page.
		// A more advanced one would open a new window with PrintableDocumentView.
		// Here we'll just trigger the browser print which respects @media print.
		triggerPrint();
		dispatch('export', { format: 'print' });
	}

	function handleKeydown(event: KeyboardEvent) {
		if ((event.ctrlKey || event.metaKey) && event.key === 's') {
			event.preventDefault();
			if (isEditing && saveStatus === 'unsaved') {
				handleSave();
			}
		}
	}

	function handleAttachmentUpload(event: CustomEvent<{ files: File[] }>) {
		dispatch('upload', event.detail);
	}

	function handleAttachmentInsert(event: CustomEvent<{ attachment: RichMarkdownAttachment }>) {
		const editor = editorComponent?.getEditor();
		if (editor) {
			insertAttachmentIntoEditor(editor, event.detail.attachment);
			// After insertion, focus back to editor
			editor.commands.focus();
		}
	}

	function handleAttachmentDelete(event: CustomEvent<{ attachment: RichMarkdownAttachment }>) {
		dispatch('delete', event.detail);
	}

	function toggleAttachments() {
		isAttachmentsOpen = !isAttachmentsOpen;
	}

	/**
	 * Public API to insert an attachment into the editor.
	 */
	export function insertAttachment(attachment: RichMarkdownAttachment) {
		handleAttachmentInsert(new CustomEvent('insert', { detail: { attachment } }));
	}
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="markdown-document-page">
	<!-- Header bar -->
	<header class="doc-header">
		<div class="doc-header-left">
			{#if showBack}
				<button class="btn btn-ghost btn-sm" on:click={handleBack} aria-label="Go back">
					<ArrowLeft size={16} />
				</button>
			{/if}
		</div>

		<div class="doc-header-center">
			{#if label}
				<span class="doc-label">{label}</span>
				<span class="doc-label-sep">·</span>
			{/if}
			<h1 class="doc-title">{title}</h1>
			{#if metadata}
				<span class="doc-meta">{metadata}</span>
			{/if}

			<!-- Color Picker -->
			{#if permissions.canEdit}
				<div class="dropdown dropdown-end ml-2">
					<button
						tabindex="0"
						class="flex h-5 w-5 items-center justify-center rounded-full border border-base-300 transition-transform hover:scale-110"
						style="background-color: {color ? `var(--rs-accent-${color})` : 'transparent'};"
						title="Set note color"
					>
						{#if !color}
							<div class="h-1.5 w-1.5 rounded-full bg-base-content/20"></div>
						{/if}
					</button>
					<ul
						class="dropdown-content menu z-[100] mt-2 w-48 rounded-xl border border-base-300 bg-base-100 p-2 shadow-xl"
					>
						<li class="menu-title text-[10px] tracking-wider text-base-content/40 uppercase">
							Purpose Color
						</li>
						<div class="grid grid-cols-4 gap-1 p-1">
							{#each PURPOSEFUL_COLORS as c}
								<button
									class="group relative flex h-8 w-full items-center justify-center rounded-lg transition-all hover:bg-base-200"
									on:click={() => {
										color = c.value;
										dispatch('save', { content: currentMarkdown || content, color: c.value });
									}}
									title={c.name}
								>
									<div
										class="h-4 w-4 rounded-full {c.class} shadow-sm transition-transform group-hover:scale-110"
										class:ring-2={color === c.value}
										class:ring-offset-2={color === c.value}
										class:ring-brand={color === c.value}
									></div>
								</button>
							{/each}
						</div>
					</ul>
				</div>
			{/if}
		</div>

		<div class="doc-header-right">
			<!-- Save status -->
			<div class="save-indicator" class:visible={isEditing}>
				{#if saveStatus === 'saving'}
					<Save size={14} class="animate-pulse" />
					<span>Saving…</span>
				{:else if saveStatus === 'saved'}
					<Check size={14} />
					<span>Saved</span>
				{:else if saveStatus === 'unsaved'}
					<span class="unsaved-dot"></span>
					<span>Unsaved</span>
				{:else if saveStatus === 'error'}
					<AlertCircle size={14} class="text-error" />
					<span class="text-error">Error</span>
				{/if}
			</div>

			<!-- Mode toggle -->
			{#if canEdit}
				<button
					class="btn btn-sm {isEditing ? 'btn-primary' : 'btn-ghost'}"
					on:click={toggleMode}
					title={isEditing ? 'Switch to read mode' : 'Switch to edit mode'}
				>
					{#if isEditing}
						<Eye size={14} />
						<span>Read</span>
					{:else}
						<Pencil size={14} />
						<span>Edit</span>
					{/if}
				</button>
			{/if}

			<!-- Save button -->
			{#if isEditing}
				<button
					class="btn btn-sm btn-primary"
					on:click={handleSave}
					disabled={saveStatus === 'saving' || saveStatus === 'saved'}
				>
					<Save size={14} />
					<span>Save</span>
				</button>
			{/if}

			<!-- Extra actions -->
			<slot name="extraActions" />

			<!-- Export & More -->
			<div class="header-actions">
				{#if permissions.canExport}
					<div class="dropdown dropdown-end">
						<button tabindex="0" class="btn btn-ghost btn-sm" aria-label="Export">
							<Download size={14} />
						</button>
						<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
						<ul
							tabindex="0"
							class="dropdown-content menu z-10 w-40 menu-sm rounded-box bg-base-200 p-1 shadow"
						>
							<li>
								<button on:click={handleExportMarkdown}>
									<FileText size={14} />
									Markdown
								</button>
							</li>
							<li>
								<button on:click={handlePrint}>
									<Download size={14} />
									Save as PDF
								</button>
							</li>
						</ul>
					</div>
				{/if}

				<div class="dropdown dropdown-end">
					<button tabindex="0" class="btn btn-ghost btn-sm" aria-label="More options">
						<MoreHorizontal size={14} />
					</button>
					<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
					<ul
						tabindex="0"
						class="dropdown-content menu z-10 w-48 menu-sm rounded-box bg-base-200 p-1 shadow"
					>
						<li>
							<button on:click={toggleAttachments}>
								<Paperclip size={14} />
								{isAttachmentsOpen ? 'Hide' : 'Show'} Attachments
								{#if attachments.length > 0}
									<span class="badge badge-sm">{attachments.length}</span>
								{/if}
							</button>
						</li>
					</ul>
				</div>
			</div>
		</div>
	</header>

	<!-- Main area with sidebar -->
	<div class="doc-main">
		<!-- Content -->
		<main class="doc-content">
			{#if isEditing}
				<RichMarkdownEditor
					bind:this={editorComponent}
					{content}
					editable={true}
					hasAttachmentHandler={true}
					bind:currentMarkdown
					on:change={handleEditorChange}
					on:attachment={toggleAttachments}
					on:sketch={(e) => dispatch('sketch', e.detail)}
					on:filedrop={handleAttachmentUpload}
				/>
			{:else}
				<RichMarkdownViewer content={currentMarkdown || content} />
			{/if}
		</main>

		<!-- Side Panel -->
		{#if isEditing || (isAttachmentsOpen && attachments.length > 0)}
			<AttachmentPanel
				{attachments}
				{permissions}
				open={isAttachmentsOpen}
				on:upload={handleAttachmentUpload}
				on:insert={handleAttachmentInsert}
				on:delete={handleAttachmentDelete}
				on:close={() => (isAttachmentsOpen = false)}
			/>
		{/if}
	</div>

	<!-- Hidden printable view for PDF generation -->
	<div class="print-only">
		<PrintableDocumentView {title} content={currentMarkdown || content} {label} />
	</div>
</div>

<style>
	.markdown-document-page {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--color-base-100, #fff);
	}

	.print-only {
		display: none;
	}

	.doc-main {
		display: flex;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.doc-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-base-300, #e5e7eb);
		min-height: 3.25rem;
		flex-shrink: 0;
	}

	.doc-header-left {
		display: flex;
		align-items: center;
		flex-shrink: 0;
	}

	.doc-header-center {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
	}

	.doc-label {
		font-size: 0.75rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.7;
		white-space: nowrap;
	}

	.doc-label-sep {
		color: var(--color-base-content, #9ca3af);
		opacity: 0.4;
	}

	.doc-title {
		font-size: 1.125rem;
		font-weight: 600;
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.doc-meta {
		font-size: 0.75rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.5;
		margin-top: 0.125rem;
		white-space: nowrap;
	}

	.doc-header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		border-left: 1px solid var(--color-base-300, #e5e7eb);
		padding-left: 0.5rem;
		margin-left: 0.25rem;
	}

	.save-indicator {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-base-content, #6b7280);
		opacity: 0;
		transition: opacity 0.2s;
		margin-right: 0.5rem;
		min-width: 70px;
	}

	.save-indicator.visible {
		opacity: 1;
	}

	.unsaved-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-warning, #f59e0b);
	}

	.doc-content {
		flex: 1;
		overflow-y: auto;
		min-height: 0;
	}

	@media (max-width: 640px) {
		.doc-header {
			flex-wrap: wrap;
			gap: 0.5rem;
		}

		.doc-header-center {
			order: 3;
			width: 100%;
		}
	}
</style>
