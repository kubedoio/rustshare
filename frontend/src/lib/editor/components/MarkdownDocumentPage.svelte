<!--
  MarkdownDocumentPage — full page wrapper that combines viewer/editor
  with title, mode toggle, save status, and action bar.
  Reusable across Notes, Decisions, Meetings and file browser.
-->
<script lang="ts">
	import { createEventDispatcher, onDestroy, tick, untrack } from 'svelte';
	import { Editor } from '@tiptap/core';
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
		ChevronRight,
		FolderInput,
		Copy,
		Trash2,
		FileCode
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
	import CollabEditor from './CollabEditor.svelte';
	import AttachmentPanel from './AttachmentPanel.svelte';
	import PrintableDocumentView from './PrintableDocumentView.svelte';
	import FilePreviewModal from '$lib/components/modals/FilePreviewModal.svelte';
	import type { PreviewableFile } from '$lib/components/modals/FilePreviewModal.svelte';
	import { insertAttachmentIntoEditor } from '../adapter/attachments';
	import { downloadTextFile, formatExportFilename, triggerPrint } from '../adapter/export';
	import { splitFrontmatter, wrapFrontmatter } from '../adapter/frontmatter';
	import { toastStore } from '$lib/stores/toast';

	const DEFAULT_AUTOSAVE_DELAY_MS = 1500;

	import { COLOR_PALETTE } from '$lib/utils/colorPalette';

	let {
		title = '',
		content = '',
		color = null,
		mode = 'read',
		permissions = READ_ONLY_PERMISSIONS,
		saveStatus = 'saved',
		revision = undefined,
		label = '',
		breadcrumb = [],
		metadata = '',
		attachments = [],
		autosaveDelay = DEFAULT_AUTOSAVE_DELAY_MS,
		showBack = true,
		embedSketchesAsBase64 = true,
		collab = false,
		docId = '',
		showNoteActions = false,
		initialAttachmentsOpen = false,
		subtitle = ''
	}: {
		title?: string;
		content?: string;
		color?: string | null;
		mode?: EditorMode;
		permissions?: EditorPermissions;
		saveStatus?: EditorSaveStatus;
		revision?: number | string;
		label?: string;
		breadcrumb?: Array<{ label: string; onClick?: () => void }>;
		metadata?: string;
		attachments?: RichMarkdownAttachment[];
		autosaveDelay?: number;
		showBack?: boolean;
		embedSketchesAsBase64?: boolean;
		collab?: boolean;
		docId?: string;
		showNoteActions?: boolean;
		initialAttachmentsOpen?: boolean;
		subtitle?: string;
	} = $props();

	const dispatch = createEventDispatcher<{
		save: { content: string; revision?: number | string; color?: string | null; docId?: string };
		modechange: { mode: EditorMode };
		back: void;
		export: { format: 'markdown' | 'print' };
		upload: { files: File[] };
		sketch: { blob: Blob; filename: string };
		delete: { attachment: RichMarkdownAttachment };
		rename: { title: string } | undefined;
		move: void;
		duplicate: void;
		deleteDocument: void;
	}>();

	interface EditorComponent {
		getMarkdown(): string;
		getEditor(): Editor | null;
		setContent(markdown: string): void;
		markSaved?(markdown?: string): void;
		markSaveError?(message?: string): void;
		flush?(): void;
	}
	let editorComponent: EditorComponent | undefined = $state();
	let isAttachmentsOpen = $state(initialAttachmentsOpen);
	let autosaveTimer: ReturnType<typeof setTimeout> | null = $state(null);
	let lastDocId = $state(docId);
	let previewAttachment = $state<RichMarkdownAttachment | null>(null);
	let showRawMarkdown = $state(false);
	let preservedFrontmatter = $state(splitFrontmatter(content).frontmatter);
	let currentMarkdown: string = $state(splitFrontmatter(content).body);

	let isTitleEditing = $state(false);
	let titleDraft = $state('');
	let titleInputRef = $state<HTMLInputElement | undefined>(undefined);

	let canEdit = $derived(permissions.canEdit);
	let isEditing = $derived(mode === 'edit' && canEdit);
	let frontmatterResult = $derived(splitFrontmatter(content));
	let hasFrontmatter = $derived(frontmatterResult.hasFrontmatter);
	let bodyContent = $derived(frontmatterResult.body);
	let frontmatterBlock = $derived(frontmatterResult.frontmatter);

	function startTitleEdit() {
		if (!canEdit || isTitleEditing) return;
		titleDraft = title;
		isTitleEditing = true;
		void tick().then(() => {
			titleInputRef?.focus();
			titleInputRef?.select();
		});
	}

	function confirmTitleEdit() {
		if (!isTitleEditing) return;
		const trimmed = titleDraft.trim();
		if (!trimmed || trimmed === title) {
			cancelTitleEdit();
			return;
		}
		isTitleEditing = false;
		dispatch('rename', { title: trimmed });
	}

	function cancelTitleEdit() {
		isTitleEditing = false;
		titleDraft = title;
	}

	function handleTitleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			confirmTitleEdit();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			cancelTitleEdit();
		}
	}

	$effect(() => {
		if (docId !== lastDocId) {
			const newDocId = docId;
			const newContent = content;
			const { frontmatter, body, hasFrontmatter: docHasFrontmatter } = splitFrontmatter(newContent);
			untrack(() => {
				preservedFrontmatter = frontmatter;
				currentMarkdown = docHasFrontmatter ? body : newContent;
				showRawMarkdown = false;
				saveStatus = 'saved';
				lastDocId = newDocId;
				isTitleEditing = false;
				titleDraft = '';
			});
		}
	});

	// Cleanup timer on unmount
	onDestroy(() => {
		if (autosaveTimer) clearTimeout(autosaveTimer);
	});

	function toggleMode() {
		const newMode: EditorMode = mode === 'read' ? 'edit' : 'read';

		// If switching from edit to read, ensure any pending autosave is flushed
		if (mode === 'edit' && editorComponent && !showRawMarkdown) {
			currentMarkdown = editorComponent.getMarkdown();
			if (collab && editorComponent && typeof editorComponent.flush === 'function') {
				editorComponent.flush();
			} else if (saveStatus === 'unsaved') {
				handleSave();
			}
		}

		if (mode === 'read') {
			currentMarkdown = currentMarkdown || (hasFrontmatter ? bodyContent : content);
		}

		mode = newMode;
		dispatch('modechange', { mode: newMode });
	}

	function toggleRawMarkdown() {
		if (!hasFrontmatter) {
			showRawMarkdown = false;
			return;
		}

		if (showRawMarkdown) {
			// Leaving raw mode: parse any frontmatter edits and edit the body only.
			const { frontmatter, body } = splitFrontmatter(currentMarkdown);
			preservedFrontmatter = frontmatter || preservedFrontmatter;
			currentMarkdown = body;
		} else {
			// Entering raw mode: expose the full Markdown including frontmatter.
			if (mode === 'edit' && editorComponent) {
				currentMarkdown = editorComponent.getMarkdown();
			}
			currentMarkdown = wrapFrontmatter(preservedFrontmatter, currentMarkdown);
		}

		showRawMarkdown = !showRawMarkdown;
	}

	function handleSave() {
		if (autosaveTimer) {
			clearTimeout(autosaveTimer);
			autosaveTimer = null;
		}

		if (saveStatus === 'saving') return;

		let md: string;
		if (showRawMarkdown) {
			md = currentMarkdown;
		} else {
			if (!editorComponent) return;
			md = editorComponent.getMarkdown();
			if (preservedFrontmatter) {
				md = wrapFrontmatter(preservedFrontmatter, md);
			}
		}

		saveStatus = 'saving';
		dispatch('save', { content: md, revision, docId });
	}

	function handleRawInput() {
		// In collab mode, CollabEditor owns the autosave trigger.
		if (collab) {
			if (saveStatus !== 'saving') {
				saveStatus = 'unsaved';
			}
			return;
		}

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

	function handleEditorChange(event: CustomEvent<{ markdown: string }>) {
		currentMarkdown = event.detail.markdown;

		// In collab mode, CollabEditor owns the autosave trigger.
		if (collab) {
			if (saveStatus !== 'saving') {
				saveStatus = 'unsaved';
			}
			return;
		}

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

	function handleCollabSave(event: CustomEvent<{ content: string; docId: string }>) {
		saveStatus = 'saving';
		dispatch('save', { content: event.detail.content, revision, docId: event.detail.docId });
	}

	export function markSaved(markdown?: string): void {
		if (editorComponent && typeof editorComponent.markSaved === 'function') {
			editorComponent.markSaved(markdown);
		}
	}

	export function markSaveError(message?: string): void {
		if (editorComponent && typeof editorComponent.markSaveError === 'function') {
			editorComponent.markSaveError(message);
		}
	}

	function handleBack() {
		dispatch('back');
	}

	function handleExportMarkdown() {
		const filename = formatExportFilename(title, 'md');
		let exportContent = currentMarkdown || content;
		if (!showRawMarkdown && preservedFrontmatter) {
			exportContent = wrapFrontmatter(preservedFrontmatter, exportContent);
		}
		downloadTextFile(filename, exportContent);
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
			if (collab) return;
			if (isEditing && saveStatus === 'unsaved') {
				handleSave();
			}
		}
	}

	function handleAttachmentUpload(event: CustomEvent<{ files: File[] }>) {
		// For documents without file storage (e.g. module items), file drop is not supported.
		// Parents that support attachments should listen for the 'upload' event.
		dispatch('upload', event.detail);
	}

	/**
	 * Handles sketch export from Excalidraw by converting the PNG blob to a
	 * base64 data URL and inserting it directly into the editor. This allows
	 * sketches to work in any markdown document even when there is no file
	 * attachment infrastructure (e.g. notes, decisions, meetings, standups).
	 */
	function handleSketch(event: CustomEvent<{ blob: Blob; filename: string }>) {
		const { blob } = event.detail;

		if (embedSketchesAsBase64) {
			const reader = new FileReader();
			reader.onload = () => {
				const dataUrl = reader.result as string;
				const editor = editorComponent?.getEditor();
				if (editor) {
					editor.chain().focus().insertContent(`![Sketch](${dataUrl})`).run();
					// Mark as unsaved so autosave triggers (only in non-collab mode)
					if (!collab) {
						if (saveStatus !== 'unsaved') {
							saveStatus = 'unsaved';
						}
						if (autosaveDelay > 0) {
							if (autosaveTimer) clearTimeout(autosaveTimer);
							autosaveTimer = setTimeout(() => {
								handleSave();
							}, autosaveDelay);
						}
					}
				}
			};
			reader.onerror = () => {
				toastStore.show('Failed to insert sketch. Please try again.', 'error');
			};
			reader.readAsDataURL(blob);
		}

		// Also dispatch to parent for any additional handling (e.g. server-side storage)
		dispatch('sketch', event.detail);
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

	function handleOpenAttachment(event: CustomEvent<{ attachment: RichMarkdownAttachment }>) {
		previewAttachment = event.detail.attachment;
	}

	function closePreview() {
		previewAttachment = null;
	}

	function toPreviewableFile(attachment: RichMarkdownAttachment): PreviewableFile {
		return {
			id: attachment.id,
			name: attachment.filename,
			mime_type: attachment.mimeType,
			size: attachment.size
		};
	}

	/**
	 * Public API to insert an attachment into the editor.
	 */
	export function insertAttachment(attachment: RichMarkdownAttachment) {
		handleAttachmentInsert(new CustomEvent('insert', { detail: { attachment } }));
	}

	/**
	 * Flushes any pending autosave without waiting for the server.
	 * Call this in beforeNavigate so saves are dispatched before destruction.
	 */
	export function flush(): void {
		if (collab && editorComponent && 'flush' in editorComponent) {
			(editorComponent as CollabEditor).flush();
		} else if (!collab && saveStatus === 'unsaved') {
			handleSave();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="markdown-document-page">
	<!-- Header bar -->
	<header class="doc-header">
		<div class="doc-header-left">
			{#if showBack}
				<button class="btn btn-ghost btn-sm" onclick={handleBack} aria-label="Go back">
					<ArrowLeft size={16} />
				</button>
			{/if}
		</div>

		<div class="doc-header-center">
			{#if label}
				<span class="doc-label">{label}</span>
				<span class="doc-label-sep">·</span>
			{/if}
			{#if isTitleEditing}
				<input
					bind:this={titleInputRef}
					type="text"
					class="doc-title-input"
					aria-label="Edit document title"
					bind:value={titleDraft}
					onkeydown={handleTitleKeydown}
					onblur={confirmTitleEdit}
				/>
			{:else}
				{#if canEdit}
					<h1 class="doc-title-wrapper">
						<button
							type="button"
							class="doc-title doc-title-button hover:opacity-80"
							onclick={startTitleEdit}
							aria-label="{title}, edit title"
						>
							{title}
						</button>
					</h1>
				{:else}
					<h1 class="doc-title">
						{title}
					</h1>
				{/if}
			{/if}
			{#if subtitle}
				<span class="doc-subtitle">{subtitle}</span>
			{/if}
			{#if metadata}
				<span class="doc-meta">{metadata}</span>
			{/if}

			<!-- Color Picker -->
			{#if permissions.canEdit && showNoteActions}
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
							<button
								class="group relative flex h-8 w-full items-center justify-center rounded-lg transition-all hover:bg-base-200"
								onclick={() => {
									color = null;
									dispatch('save', { content: currentMarkdown || content, color: null });
								}}
								title="Default"
							>
								<div
									class="h-4 w-4 rounded-full bg-base-300 shadow-sm transition-transform group-hover:scale-110"
									class:ring-2={color === null}
									class:ring-offset-2={color === null}
									class:ring-brand={color === null}
								></div>
							</button>
							{#each COLOR_PALETTE as c}
								<button
									class="group relative flex h-8 w-full items-center justify-center rounded-lg transition-all hover:bg-base-200"
									onclick={() => {
										color = c.key;
										dispatch('save', { content: currentMarkdown || content, color: c.key });
									}}
									title={c.label}
								>
									<div
										class="h-4 w-4 rounded-full {c.editorClass} shadow-sm transition-transform group-hover:scale-110"
										class:ring-2={color === c.key}
										class:ring-offset-2={color === c.key}
										class:ring-brand={color === c.key}
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
					onclick={toggleMode}
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

				{#if hasFrontmatter}
					<button
						class="btn btn-sm {showRawMarkdown ? 'btn-primary' : 'btn-ghost'}"
						onclick={toggleRawMarkdown}
						title={showRawMarkdown ? 'Switch to rich editor' : 'Edit raw Markdown'}
					>
						<FileCode size={14} />
						<span>{showRawMarkdown ? 'Rich' : 'Raw'}</span>
					</button>
				{/if}
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
								<button onclick={handleExportMarkdown}>
									<FileText size={14} />
									Markdown
								</button>
							</li>
							<li>
								<button onclick={handlePrint}>
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
						class="dropdown-content menu z-10 w-52 menu-sm rounded-box bg-base-200 p-1 shadow"
					>
						<li>
							<button onclick={toggleAttachments}>
								<Paperclip size={14} />
								{isAttachmentsOpen ? 'Hide' : 'Show'} Attachments
								{#if attachments.length > 0}
									<span class="badge badge-sm">{attachments.length}</span>
								{/if}
							</button>
						</li>
						{#if showNoteActions}
							<li>
								<button onclick={() => dispatch('rename')}>
									<Pencil size={14} />
									Rename note
								</button>
							</li>
							<li>
								<button onclick={() => dispatch('move')}>
									<FolderInput size={14} />
									Move to folder
								</button>
							</li>
							<li>
								<button onclick={() => dispatch('duplicate')}>
									<Copy size={14} />
									Duplicate note
								</button>
							</li>
							<li>
								<button onclick={() => dispatch('deleteDocument')} class="text-error">
									<Trash2 size={14} />
									Delete note
								</button>
							</li>
						{/if}
					</ul>
				</div>
			</div>
		</div>
	</header>

	<!-- Breadcrumb -->
	{#if breadcrumb.length > 0}
		<nav aria-label="Breadcrumb" class="doc-breadcrumb">
			{#each breadcrumb as crumb, i}
				{#if crumb.onClick}
					<button class="btn btn-ghost btn-xs" onclick={crumb.onClick}>
						{crumb.label}
					</button>
				{:else}
					<span class="text-xs font-medium text-base-content/70">
						{crumb.label}
					</span>
				{/if}
				{#if i < breadcrumb.length - 1}
					<ChevronRight size={12} class="text-base-content/30" />
				{/if}
			{/each}
		</nav>
	{/if}

	<!-- Main area with sidebar -->
	<div class="doc-main">
		<!-- Content -->
		<main class="doc-content">
			{#if isEditing}
				{#if showRawMarkdown}
					<textarea
						class="raw-markdown-editor"
						bind:value={currentMarkdown}
						oninput={handleRawInput}
						aria-label="Raw Markdown editor"></textarea>
				{:else if collab && docId}
					{#key docId}
						<CollabEditor
							bind:this={editorComponent}
							{docId}
							content={hasFrontmatter ? bodyContent : content}
							editable={true}
							hasAttachmentHandler={true}
							currentMarkdown={currentMarkdown || bodyContent || content}
							on:change={handleEditorChange}
							on:save={handleCollabSave}
							on:ready
							on:attachment={toggleAttachments}
							on:sketch={handleSketch}
							on:filedrop={handleAttachmentUpload}
							on:paste={handleAttachmentUpload}
						/>
					{/key}
				{:else}
					{#key docId}
						<RichMarkdownEditor
							bind:this={editorComponent}
							content={hasFrontmatter ? bodyContent : content}
							editable={true}
							hasAttachmentHandler={true}
							syncExternalContent={false}
							currentMarkdown={currentMarkdown || bodyContent || content}
							on:change={handleEditorChange}
							on:attachment={toggleAttachments}
							on:sketch={handleSketch}
							on:filedrop={handleAttachmentUpload}
							on:paste={handleAttachmentUpload}
						/>
					{/key}
				{/if}
			{:else if showRawMarkdown}
				<pre class="raw-markdown-viewer" aria-label="Raw Markdown"><code>{currentMarkdown}</code
					></pre>
			{:else}
				<RichMarkdownViewer
					content={currentMarkdown || bodyContent || content}
					{attachments}
					on:open={handleOpenAttachment}
				/>
			{/if}
		</main>

		<!-- Side Panel -->
		{#if isEditing || (isAttachmentsOpen && attachments.length > 0)}
			<AttachmentPanel
				{attachments}
				{permissions}
				open={isAttachmentsOpen}
				editable={isEditing}
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

	{#if previewAttachment}
		<FilePreviewModal
			open={true}
			file={toPreviewableFile(previewAttachment)}
			onClose={closePreview}
		/>
	{/if}
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
		white-space: nowrap;
		max-width: 100%;
	}

	.doc-title-wrapper {
		margin: 0;
		min-width: 0;
		max-width: 100%;
	}

	.doc-title-button {
		background: transparent;
		border: none;
		padding: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
		max-width: 100%;
	}

	.doc-title-input {
		font-size: 1.125rem;
		font-weight: 600;
		background: transparent;
		border: none;
		border-bottom: 2px solid var(--rs-brand-500, #3b82f6);
		color: inherit;
		min-width: 120px;
		max-width: 400px;
		padding: 0;
		margin: 0;
		outline: none;
	}

	.doc-subtitle {
		font-size: 0.75rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.6;
		margin-top: 0.125rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 400px;
	}

	.doc-meta {
		font-size: 0.75rem;
		color: var(--color-base-content, #9ca3af);
		opacity: 0.5;
		margin-top: 0.125rem;
		white-space: nowrap;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.doc-header-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.doc-breadcrumb {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--color-base-300, #e5e7eb);
		background: transparent;
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

	.raw-markdown-editor {
		width: 100%;
		height: 100%;
		resize: none;
		padding: 1rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.875rem;
		line-height: 1.5;
		background: var(--color-base-100, #fff);
		color: var(--color-base-content, #1f2937);
		border: none;
		outline: none;
	}

	.raw-markdown-viewer {
		width: 100%;
		height: 100%;
		margin: 0;
		padding: 1rem;
		overflow-y: auto;
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 0.875rem;
		line-height: 1.5;
		background: var(--color-base-100, #fff);
		color: var(--color-base-content, #1f2937);
		white-space: pre-wrap;
	}

	.raw-markdown-viewer code {
		font-family: inherit;
	}

	@media (max-width: 640px) {
		.doc-header {
			flex-wrap: wrap;
			gap: 0.5rem;
		}

		.doc-header-center {
			order: 3;
			width: 100%;
			flex-wrap: wrap;
		}

		/* Keep every header action reachable: the actions group may wrap onto
		   its own line instead of being clipped off the viewport edge. */
		.doc-header-right {
			flex: 1 1 auto;
			min-width: 0;
			flex-wrap: wrap;
			justify-content: flex-end;
		}

		/* The save indicator reserves 70px even when hidden; reclaim it so the
		   back button and actions fit on the first row. */
		.save-indicator:not(.visible) {
			display: none;
		}

		.save-indicator {
			min-width: 0;
			margin-right: 0;
		}

		.header-actions {
			border-left: none;
			padding-left: 0;
			margin-left: 0;
		}
	}
</style>
