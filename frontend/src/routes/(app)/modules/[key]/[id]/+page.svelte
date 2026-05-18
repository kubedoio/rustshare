<script lang="ts">
	import { untrack } from 'svelte';
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import { notesApi, renameNote, moveNote, deleteNote, duplicateNote } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { meetingsApi } from '$lib/api/meetings';
	import { standupsApi } from '$lib/api/standups';
	import { uploadFile, deleteFile } from '$lib/api/files';
	import { getFolderContents } from '$lib/api/folders';
	import { getModuleByKey } from '$lib/modules/registry';
	import { goto } from '$app/navigation';
	import { Folder, Share2, Pencil } from 'lucide-svelte';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import MoveModal from '$lib/components/modals/MoveModal.svelte';
	import DeleteConfirmation from '$lib/components/modals/DeleteConfirmation.svelte';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import { toastStore } from '$lib/stores/toast';
	import type { EditorMode, EditorSaveStatus, RichMarkdownAttachment } from '$lib/editor/types';
	import { classifyAttachmentKind } from '$lib/editor/validation';
	import {
		resolveAttachmentPaths,
		restoreRelativePaths,
		generateUniqueFilename
	} from '$lib/editor/adapter/attachments';

	let key = $derived(($page.params.key || '') as string);
	let id = $derived(($page.params.id || '') as string);
	let module = $derived(getModuleByKey(key));

	// Determine which API to use
	let api = $derived(
		key === 'notes'
			? notesApi
			: key === 'decisions'
				? decisionsApi
				: key === 'meetings'
					? meetingsApi
					: key === 'standups'
						? standupsApi
						: null
	);

	function currentKey() {
		return $page.params.key || '';
	}

	function currentId() {
		return $page.params.id || '';
	}

	function currentApi() {
		const moduleKey = currentKey();
		if (moduleKey === 'notes') return notesApi;
		if (moduleKey === 'decisions') return decisionsApi;
		if (moduleKey === 'meetings') return meetingsApi;
		if (moduleKey === 'standups') return standupsApi;
		return null;
	}

	const query = createQuery<any, Error, any, any, string[]>({
		queryKey: ['module-item', currentKey(), currentId()],
		queryFn: () => currentApi()?.get(currentId()),
		enabled: !!currentApi() && !!currentId()
	});

	let item = $derived($query.data);
	let content = $derived(item?.content ?? '');
	let title = $derived(item?.metadata?.title || item?.name || '');
	let modifiedAt = $derived(
		item?.modified_at
			? key === 'meetings' && item?.metadata?.date
				? `Date: ${new Date(item.metadata.date).toLocaleDateString()}${item.metadata.attendees?.length ? ` • ${item.metadata.attendees.length} attendee${item.metadata.attendees.length === 1 ? '' : 's'}` : ''} • Last edited ${new Date(item.modified_at).toLocaleString()}`
				: key === 'decisions' && item?.name?.match(/^DEC-\d+/)
					? `${item.name.match(/^DEC-\d+/)?.[0]} • Last edited ${new Date(item.modified_at).toLocaleString()}`
					: `Last edited ${new Date(item.modified_at).toLocaleString()}`
			: ''
	);
	let mode: EditorMode = $state(currentKey() === 'notes' ? 'edit' : 'read');
	let saveStatus: EditorSaveStatus = $state('saved');
	let showShareModal = $state(false);
	let showRenameModal = $state(false);
	let renameError = $state('');
	let isRenaming = $state(false);
	let showMoveModal = $state(false);
	let isMoving = $state(false);
	let showDeleteModal = $state(false);
	let isDeleting = $state(false);
	let isDuplicating = $state(false);
	let attachments = $state<RichMarkdownAttachment[]>([]);
	let documentPage = $state<MarkdownDocumentPage | undefined>(undefined);

	let isFolderBacked = $derived(item?.name === 'note.md');

	$effect(() => {
		if (item?.metadata?.attachments) {
			const serverAttachments = item.metadata.attachments.map((a: any) => {
				const isImage = a.mime_type?.startsWith('image/');
				// For folder-backed notes, use relative paths so markdown stays portable
				const path = isFolderBacked
					? `attachments/${a.name}`
					: isImage
						? `/api/v1/files/${a.file_id}/preview`
						: `/api/v1/files/${a.file_id}/content`;
				return {
					id: a.file_id,
					filename: a.name,
					path,
					mimeType: a.mime_type,
					size: a.size,
					kind: classifyAttachmentKind(a.mime_type),
					createdAt: a.created_at,
					createdBy: ''
				};
			});
			// Preserve local-only attachments that haven't been saved yet
			// (prevents background refetch from dropping unsaved uploads)
			const serverIds = new Set(serverAttachments.map((a: RichMarkdownAttachment) => a.id));
			const localOnly = untrack(() => attachments).filter((a) => !serverIds.has(a.id));
			attachments = [...serverAttachments, ...localOnly];
		} else if (untrack(() => attachments).length === 0) {
			attachments = [];
		}
	});

	// Preprocess content for editor/viewer: resolve relative paths to API URLs
	let editorContent = $derived(
		isFolderBacked && attachments.length > 0
			? resolveAttachmentPaths(item?.content ?? '', attachments)
			: (item?.content ?? '')
	);

	let breadcrumb = $derived([
		{ label: module?.displayName || key, onClick: () => goto(`/modules/${key}`) },
		{ label: title }
	]);

	function serializeNoteAttachments() {
		return attachments.map((a) => ({
			file_id: a.id,
			name: a.filename,
			mime_type: a.mimeType,
			size: a.size,
			created_at: a.createdAt
		}));
	}

	const saveMutation = createMutation<any, Error, { title: string; content: string }>({
		mutationFn: (data: { title: string; content: string }) => {
			const noteAttachments = serializeNoteAttachments();
			if (key === 'notes')
				return notesApi.update(id, { content: data.content, attachments: noteAttachments });
			if (key === 'decisions')
				return decisionsApi.update(id, { title: data.title, content: data.content });
			if (key === 'meetings')
				return meetingsApi.update(id, { title: data.title, content: data.content });
			if (key === 'standups')
				return standupsApi.update(id, { title: data.title, content: data.content });
			return Promise.reject('Invalid module');
		}
	});

	async function handleSave(event: CustomEvent<{ content: string; docId?: string }>) {
		if (event.detail.docId && event.detail.docId !== currentId()) {
			return;
		}

		saveStatus = 'saving';
		const editorContent = event.detail.content;
		// Postprocess: convert API URLs back to relative paths for folder-backed notes
		let saveContent = editorContent;
		if (isFolderBacked && attachments.length > 0) {
			saveContent = restoreRelativePaths(saveContent, attachments);
		}
		try {
			const saved = await $saveMutation.mutateAsync({ title, content: saveContent });
			saveStatus = 'saved';
			documentPage?.markSaved(editorContent);
			if (key === 'notes') {
				const noteAttachments = serializeNoteAttachments();
				queryClient.setQueryData(['module-item', currentKey(), currentId()], (previous: any) => {
					if (!previous) return previous;
					const modifiedAt = saved?.modified_at ?? previous.modified_at;
					return {
						...previous,
						content: saveContent,
						current_version: saved?.current_version ?? previous.current_version,
						modified_at: modifiedAt,
						metadata: {
							...previous.metadata,
							attachments: noteAttachments,
							updated_at: modifiedAt
						}
					};
				});
			} else {
				await $query.refetch();
			}
		} catch (error) {
			saveStatus = 'error';
			documentPage?.markSaveError(error instanceof Error ? error.message : 'Autosave failed');
		}
	}

	function handleBack() {
		goto(`/modules/${key}`);
	}

	function handleModeChange(event: CustomEvent<{ mode: EditorMode }>) {
		mode = event.detail.mode;
	}

	async function handleUpload(event: CustomEvent<{ files: File[] }>) {
		if (!item) return;
		if (!item.parent_folder_id) {
			toastStore.show('This item must be saved to a folder before adding attachments', 'error');
			return;
		}

		// For folder-backed notes (note.md), upload to the attachments/ subfolder
		let uploadFolderId = item.parent_folder_id;
		if (isFolderBacked) {
			try {
				const contents = await getFolderContents(item.parent_folder_id);
				const attachmentsFolder = contents.folders?.find((f: any) => f.name === 'attachments');
				if (attachmentsFolder) {
					uploadFolderId = attachmentsFolder.id;
				}
			} catch (err) {
				console.warn('Could not resolve attachments subfolder, uploading to bundle root:', err);
			}
		}

		for (const file of event.detail.files) {
			try {
				// Collision-safe filename for folder-backed notes
				let uploadName: string | undefined;
				if (isFolderBacked && uploadFolderId) {
					try {
						const folderContents = await getFolderContents(uploadFolderId);
						const existingNames = folderContents.files?.map((f: any) => f.name) || [];
						uploadName = generateUniqueFilename(file.name, existingNames);
					} catch (err) {
						console.warn('Could not check for filename collisions:', err);
					}
				}

				const uploaded = await uploadFile(uploadFolderId, file, undefined, uploadName);
				const isImage = uploaded.mime_type?.startsWith('image/');
				const relativePath = isFolderBacked ? `attachments/${uploaded.name}` : undefined;
				const apiUrl = isImage
					? `/api/v1/files/${uploaded.id}/preview`
					: `/api/v1/files/${uploaded.id}/content`;
				const attachment: RichMarkdownAttachment = {
					id: uploaded.id,
					filename: uploaded.name,
					path: relativePath || apiUrl,
					mimeType: uploaded.mime_type,
					size: uploaded.size,
					kind: classifyAttachmentKind(uploaded.mime_type),
					createdAt: uploaded.created_at,
					createdBy: ''
				};
				attachments = [...attachments, attachment];

				// Auto-insert relative link into editor for folder-backed notes
				if (isFolderBacked && documentPage) {
					documentPage.insertAttachment(attachment);
				}
			} catch (err) {
				console.error('Failed to upload attachment:', err);
				toastStore.show('Failed to upload attachment', 'error');
			}
		}
	}

	async function handleDeleteAttachment(
		event: CustomEvent<{ attachment: RichMarkdownAttachment }>
	) {
		try {
			await deleteFile(event.detail.attachment.id);
			attachments = attachments.filter((a) => a.id !== event.detail.attachment.id);
		} catch (err) {
			console.error('Failed to delete attachment:', err);
			toastStore.show('Failed to delete attachment', 'error');
		}
	}

	async function handleSketch(event: CustomEvent<{ blob: Blob; filename: string }>) {
		if (!item || !item.parent_folder_id) return;

		if (isFolderBacked) {
			// Upload sketch PNG to drawings/ subfolder
			try {
				const contents = await getFolderContents(item.parent_folder_id);
				const drawingsFolder = contents.folders?.find((f: any) => f.name === 'drawings');
				if (drawingsFolder) {
					const folderContents = await getFolderContents(drawingsFolder.id);
					const existingNames = folderContents.files?.map((f: any) => f.name) || [];
					const uploadName = generateUniqueFilename(event.detail.filename, existingNames);
					const sketchFile = new File([event.detail.blob], uploadName, { type: 'image/png' });
					const uploaded = await uploadFile(drawingsFolder.id, sketchFile, undefined, uploadName);
					const attachment: RichMarkdownAttachment = {
						id: uploaded.id,
						filename: uploaded.name,
						path: `drawings/${uploaded.name}`,
						mimeType: 'image/png',
						size: uploaded.size,
						kind: 'image',
						createdAt: uploaded.created_at,
						createdBy: ''
					};
					attachments = [...attachments, attachment];
					if (documentPage) {
						documentPage.insertAttachment(attachment);
					}
					return;
				}
			} catch (err) {
				console.warn('Failed to upload sketch to drawings folder:', err);
			}
		}

		// Fallback for legacy notes: base64 embedding is handled by MarkdownDocumentPage
	}

	async function handleOpenInFiles() {
		if (module?.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function handleShareNotification(event: { message: string; type: 'success' | 'error' | 'info' }) {
		toastStore.show(event.message, event.type);
	}

	async function handleRenameConfirm(newTitle: string) {
		if (isRenaming) return;
		const trimmed = newTitle.trim();
		if (!trimmed) {
			renameError = 'Title is required';
			return;
		}
		isRenaming = true;
		renameError = '';
		try {
			if (key === 'notes') {
				await renameNote(id, { title: trimmed });
				toastStore.show('Note renamed', 'success');
			} else if (key === 'decisions') {
				await decisionsApi.rename(id, { title: trimmed });
				toastStore.show('Decision renamed', 'success');
			}
			showRenameModal = false;
			renameError = '';
			$query.refetch();
		} catch (err) {
			console.error('Failed to rename:', err);
			renameError = err instanceof Error ? err.message : 'Failed to rename';
		} finally {
			isRenaming = false;
		}
	}

	async function handleMoveConfirm(payload: { targetFolderId: string | null }) {
		if (isMoving || !item) return;
		isMoving = true;
		try {
			if (key === 'notes') {
				await moveNote(id, { target_folder_id: payload.targetFolderId });
				toastStore.show('Note moved', 'success');
			}
			showMoveModal = false;
			$query.refetch();
		} catch (err) {
			console.error('Failed to move:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to move', 'error');
		} finally {
			isMoving = false;
		}
	}

	async function handleDuplicate() {
		if (isDuplicating || !item) return;
		isDuplicating = true;
		try {
			const duplicated = await duplicateNote(id);
			toastStore.show('Note duplicated', 'success');
			goto(`/modules/notes/${duplicated.id}`);
		} catch (err) {
			console.error('Failed to duplicate note:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to duplicate note', 'error');
		} finally {
			isDuplicating = false;
		}
	}

	async function handleDeleteConfirm() {
		if (isDeleting || !item) return;
		isDeleting = true;
		try {
			if (key === 'notes') {
				await deleteNote(id);
				toastStore.show('Note deleted', 'success');
			}
			showDeleteModal = false;
			goto(`/modules/${key}`);
		} catch (err) {
			console.error('Failed to delete:', err);
			toastStore.show(err instanceof Error ? err.message : 'Failed to delete', 'error');
		} finally {
			isDeleting = false;
		}
	}
</script>

<div class="module-detail-page h-full">
	{#if $query.isLoading}
		<div class="flex h-full items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else if $query.error}
		<div class="flex h-full flex-col items-center justify-center p-8 text-center">
			<p class="text-error">Failed to load item.</p>
			<button class="btn mt-4 btn-ghost" onclick={() => $query.refetch()}>Retry</button>
		</div>
	{:else if item}
		<MarkdownDocumentPage
			bind:this={documentPage}
			{title}
			content={editorContent}
			{mode}
			{saveStatus}
			{breadcrumb}
			{attachments}
			metadata={modifiedAt}
			permissions={{
				canRead: true,
				canEdit: true,
				canUploadAttachments: true,
				canDeleteAttachments: true,
				canExport: true,
				canShare: true
			}}
			embedSketchesAsBase64={!isFolderBacked}
			collab={key === 'notes'}
			docId={id}
			showNoteActions={key === 'notes'}
			on:save={handleSave}
			on:back={handleBack}
			on:modechange={handleModeChange}
			on:upload={handleUpload}
			on:delete={handleDeleteAttachment}
			on:sketch={handleSketch}
			on:rename={() => { showRenameModal = true; renameError = ''; }}
			on:move={() => { showMoveModal = true; }}
			on:duplicate={handleDuplicate}
			on:deleteDocument={() => { showDeleteModal = true; }}
		>
			<svelte:fragment slot="extraActions">
				{#if key === 'notes'}
					<button class="btn gap-1.5 btn-ghost btn-sm" onclick={() => (showShareModal = true)}>
						<Share2 size={14} />
						<span>Share</span>
					</button>
				{/if}

				{#if key === 'decisions'}
					<button
						class="btn gap-1.5 btn-ghost btn-sm"
						onclick={() => {
							showRenameModal = true;
							renameError = '';
						}}
					>
						<Pencil size={14} />
						<span>Rename</span>
					</button>
				{/if}

				<button class="btn gap-1.5 btn-ghost btn-sm" onclick={handleOpenInFiles}>
					<Folder size={14} />
					<span>Open in Files</span>
				</button>
			</svelte:fragment>
		</MarkdownDocumentPage>

		<ShareModal
			open={showShareModal}
			resourceId={item.id}
			resourceName={title}
			resourceType="file"
			initialTab="share"
			onClose={() => (showShareModal = false)}
			onNotification={handleShareNotification}
		/>

		{#if key === 'notes' || key === 'decisions'}
			<PromptModal
				open={showRenameModal}
				title={key === 'notes' ? 'Rename note' : 'Rename decision'}
				message="New title"
				defaultValue={title}
				confirmLabel="Rename"
				error={renameError}
				isLoading={isRenaming}
				onConfirm={handleRenameConfirm}
				onCancel={() => {
					showRenameModal = false;
					renameError = '';
				}}
			/>
		{/if}

		{#if key === 'notes'}
			<MoveModal
				open={showMoveModal}
				loading={isMoving}
				itemName={title}
				itemType="file"
				currentFolderId={item?.parent_folder_id ?? null}
				itemId={id}
				onClose={() => (showMoveModal = false)}
				onConfirm={handleMoveConfirm}
			/>

			<DeleteConfirmation
				open={showDeleteModal}
				loading={isDeleting}
				itemName={title}
				itemType="file"
				onClose={() => (showDeleteModal = false)}
				onConfirm={handleDeleteConfirm}
			/>
		{/if}
	{/if}
</div>

<style>
	.module-detail-page {
		background: var(--rs-bg);
		height: 100%;
	}
</style>
