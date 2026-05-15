<!--
  AttachmentPanel — side panel for managing document attachments.
  Shows upload zone, file list with insert/delete actions.
  Upload is delegated to parent via events.
-->
<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import {
		Upload,
		Image,
		FileText,
		File as FileIcon,
		Table2,
		Archive,
		Trash2,
		Plus,
		X,
		AlertCircle
	} from 'lucide-svelte';
	import type { RichMarkdownAttachment, EditorPermissions } from '../types';
	import {
		validateAttachmentUpload,
		filterVisibleAttachments,
		formatFileSize,
		isInlineableImage
	} from '../adapter/attachments';

	/** Current attachment list */
	export let attachments: RichMarkdownAttachment[] = [];

	/** User permissions */
	export let permissions: EditorPermissions;

	/** Whether the panel is visible */
	export let open: boolean = false;

	/** Whether the editor is in edit mode (controls insert button visibility) */
	export let editable: boolean = true;

	const dispatch = createEventDispatcher<{
		upload: { files: File[] };
		insert: { attachment: RichMarkdownAttachment };
		delete: { attachment: RichMarkdownAttachment };
		close: void;
	}>();

	let fileInput: HTMLInputElement;
	let isDragOver = false;
	let error: string | null = null;

	$: visibleAttachments = filterVisibleAttachments(attachments);
	$: canUpload = permissions.canUploadAttachments;
	$: canDelete = permissions.canDeleteAttachments;

	const KIND_ICONS: Record<string, typeof FileText> = {
		image: Image,
		pdf: FileText,
		document: FileText,
		spreadsheet: Table2,
		archive: Archive,
		other: FileIcon
	};

	function getKindIcon(kind: string) {
		return KIND_ICONS[kind] || FileIcon;
	}

	function handleFileSelect(event: Event) {
		const input = event.target as HTMLInputElement;
		if (!input.files?.length) return;
		processFiles(Array.from(input.files));
		input.value = '';
	}

	function handleDrop(event: DragEvent) {
		event.preventDefault();
		isDragOver = false;
		if (!canUpload) return;

		const files = event.dataTransfer?.files;
		if (files?.length) {
			processFiles(Array.from(files));
		}
	}

	function handleDragOver(event: DragEvent) {
		event.preventDefault();
		if (canUpload) isDragOver = true;
	}

	function handleDragLeave() {
		isDragOver = false;
	}

	function processFiles(files: File[]) {
		error = null;

		// Validate each file
		const validFiles: File[] = [];
		for (const file of files) {
			const result = validateAttachmentUpload(file, {
				permissions,
				existingAttachments: attachments
			});
			if (result.valid) {
				validFiles.push(file);
			} else {
				error = result.error || 'Invalid file';
			}
		}

		if (validFiles.length > 0) {
			dispatch('upload', { files: validFiles });
		}
	}

	function handleInsert(attachment: RichMarkdownAttachment) {
		dispatch('insert', { attachment });
	}

	function handleDelete(attachment: RichMarkdownAttachment) {
		dispatch('delete', { attachment });
	}
</script>

{#if open}
	<div class="attachment-panel">
		<div class="panel-header">
			<h3 class="panel-title">Attachments</h3>
			<span class="panel-count">{visibleAttachments.length}</span>
			<button class="panel-close" on:click={() => dispatch('close')} aria-label="Close panel">
				<X size={16} />
			</button>
		</div>

		<!-- Upload zone -->
		{#if canUpload}
			<div
				class="upload-zone"
				class:drag-over={isDragOver}
				on:drop={handleDrop}
				on:dragover={handleDragOver}
				on:dragleave={handleDragLeave}
				role="button"
				tabindex="0"
				on:click={() => fileInput.click()}
				on:keydown={(e) => e.key === 'Enter' && fileInput.click()}
			>
				<Upload size={20} />
				<span class="upload-label">
					{isDragOver ? 'Drop files here' : 'Click or drag files'}
				</span>
				<input
					bind:this={fileInput}
					type="file"
					multiple
					class="upload-input"
					on:change={handleFileSelect}
					aria-label="Upload attachments"
				/>
			</div>
		{/if}

		<!-- Error -->
		{#if error}
			<div class="panel-error">
				<AlertCircle size={14} />
				<span>{error}</span>
			</div>
		{/if}

		<!-- File list -->
		<div class="attachment-list">
			{#if visibleAttachments.length === 0}
				<div class="empty-state">
					<p>No attachments yet</p>
				</div>
			{:else}
				{#each visibleAttachments as attachment (attachment.id)}
					<div class="attachment-item">
						<span class="attachment-icon">
							<svelte:component this={getKindIcon(attachment.kind)} size={16} />
						</span>
						<div class="attachment-info">
							<span class="attachment-name" title={attachment.filename}>
								{attachment.filename}
							</span>
							<span class="attachment-meta">{formatFileSize(attachment.size)}</span>
						</div>
						<div class="attachment-actions">
							{#if editable}
								<button
									class="action-btn"
									on:click={() => handleInsert(attachment)}
									title={isInlineableImage(attachment.mimeType) ? 'Insert image' : 'Insert link'}
									aria-label="Insert into editor"
								>
									<Plus size={14} />
								</button>
							{/if}
							{#if canDelete}
								<button
									class="action-btn action-btn-danger"
									on:click={() => handleDelete(attachment)}
									title="Delete attachment"
									aria-label="Delete attachment"
								>
									<Trash2 size={14} />
								</button>
							{/if}
						</div>
					</div>
				{/each}
			{/if}
		</div>
	</div>
{/if}

<style>
	.attachment-panel {
		width: 280px;
		border-left: 1px solid var(--color-base-300, #e5e7eb);
		background: var(--color-base-100, #fff);
		display: flex;
		flex-direction: column;
		height: 100%;
		flex-shrink: 0;
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-base-300, #e5e7eb);
	}

	.panel-title {
		font-size: 0.8125rem;
		font-weight: 600;
		margin: 0;
	}

	.panel-count {
		font-size: 0.6875rem;
		background: var(--color-base-200, #f3f4f6);
		padding: 0.125rem 0.375rem;
		border-radius: 0.75rem;
		color: var(--color-base-content, #6b7280);
	}

	.panel-close {
		margin-left: auto;
		padding: 0.25rem;
		border: none;
		background: transparent;
		cursor: pointer;
		color: var(--color-base-content, #9ca3af);
		border-radius: 0.25rem;
	}

	.panel-close:hover {
		background: var(--color-base-200, #f3f4f6);
	}

	.upload-zone {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 1rem;
		margin: 0.75rem;
		border: 2px dashed var(--color-base-300, #d1d5db);
		border-radius: 0.5rem;
		cursor: pointer;
		transition:
			border-color 0.15s,
			background 0.15s;
		color: var(--color-base-content, #9ca3af);
		position: relative;
	}

	.upload-zone:hover,
	.upload-zone.drag-over {
		border-color: var(--color-primary, #3b82f6);
		background: color-mix(in oklab, var(--color-primary, #3b82f6) 5%, transparent);
	}

	.upload-label {
		font-size: 0.75rem;
	}

	.upload-input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	.panel-error {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.5rem 0.75rem;
		margin: 0 0.75rem;
		background: color-mix(in oklab, var(--color-error, #ef4444) 10%, transparent);
		border-radius: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-error, #ef4444);
	}

	.attachment-list {
		flex: 1;
		overflow-y: auto;
		padding: 0.5rem;
	}

	.empty-state {
		text-align: center;
		padding: 1.5rem 1rem;
		font-size: 0.75rem;
		color: var(--color-base-content, #9ca3af);
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem;
		border-radius: 0.375rem;
		transition: background 0.1s;
	}

	.attachment-item:hover {
		background: var(--color-base-200, #f3f4f6);
	}

	.attachment-icon {
		display: flex;
		align-items: center;
		color: var(--color-base-content, #6b7280);
		flex-shrink: 0;
	}

	.attachment-info {
		flex: 1;
		min-width: 0;
	}

	.attachment-name {
		display: block;
		font-size: 0.8125rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-meta {
		font-size: 0.6875rem;
		color: var(--color-base-content, #9ca3af);
	}

	.attachment-actions {
		display: flex;
		gap: 0.125rem;
		opacity: 0;
		transition: opacity 0.1s;
	}

	.attachment-item:hover .attachment-actions {
		opacity: 1;
	}

	.action-btn {
		padding: 0.25rem;
		border: none;
		background: transparent;
		cursor: pointer;
		color: var(--color-base-content, #6b7280);
		border-radius: 0.25rem;
	}

	.action-btn:hover {
		background: var(--color-base-300, #d1d5db);
	}

	.action-btn-danger:hover {
		background: color-mix(in oklab, var(--color-error, #ef4444) 15%, transparent);
		color: var(--color-error, #ef4444);
	}

	@media (max-width: 768px) {
		.attachment-panel {
			position: absolute;
			right: 0;
			top: 0;
			z-index: 40;
			box-shadow: -4px 0 16px rgba(0, 0, 0, 0.1);
		}
	}
</style>
