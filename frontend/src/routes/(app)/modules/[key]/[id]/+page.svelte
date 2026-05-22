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
	import { goto, beforeNavigate } from '$app/navigation';
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
	import type { NoteAttachment, NoteMetadata, Folder as ApiFolder, File as ApiFile } from '$lib/api/types';

	const ATTACHMENTS_QUERY_PARAM = 'open';
	const NOTE_MD_FILENAME = 'note.md';
	const ATTACHMENTS_FOLDER_NAME = 'attachments';
	const DRAWINGS_FOLDER_NAME = 'drawings';

	interface ModuleApi {
		get: (id: string) => Promise<unknown>;
	}

	const MODULE_API_MAP: Record<string, ModuleApi> = {
		notes: notesApi,
		decisions: decisionsApi,
		meetings: meetingsApi,
		standups: standupsApi
	};

	type ModuleItemQueryData = {
		content: string;
		modified_at: string;
		current_version: number;
		metadata: NoteMetadata & { attachments: NoteAttachment[] };
	};

	interface ModuleItem {
		id: string;
		name?: string;
		content: string;
		metadata?: {
			title?: string;
			excerpt?: string;
			attachments?: NoteAttachment[];
			date?: string;
			attendees?: string[];
		};
		modified_at?: string;
		parent_folder_id?: string | null;
	}

	let key = $derived(($page.params.key || '') as string);
	let id = $derived(($page.params.id || '') as string);
	let module = $derived(getModuleByKey(key));
	let initialAttachmentsOpen = $derived(
		$page.url.searchParams.get('attachments') === ATTACHMENTS_QUERY_PARAM
	);

	let api = $derived(MODULE_API_MAP[key] ?? null);

	const query = createQuery<unknown, Error, unknown, unknown, string[]>({
		queryKey: ['module-item', key, id],
		queryFn: () => api?.get(id),
		enabled: !!api && !!id
	});

	$effect(() => {
		query.setOptions({
			queryKey: ['module-item', key, id],
			queryFn: () => api?.get(id),
			enabled: !!api && !!id
		});
	});

	let item = $derived($query.data as ModuleItem | undefined);
	let content = $derived(item?.content ?? '');
	let title = $derived(item?.metadata?.title || item?.name || '');
	let subtitle = $derived(key === 'notes' ? (item?.metadata?.excerpt || '') : '');
	let modifiedAt = $derived(
		item?.modified_at
			? key === 'meetings' && item?.metadata?.date
					? `Date: ${new Date(item.metadata.date).toLocaleDateString()}${item.metadata.attendees?.length ? ` • ${item.metadata.attendees.length} attendee${item.metadata.attendees.length === 1 ? '' : 's'}` : ''} • Last edited ${new Date(item.modified_at).toLocaleString()}`
					: key === 'decisions' && item?.name?.match(/^DEC-\d+/)
							? `${item.name.match(/^DEC-\d+/)?.[0]} • Last edited ${new Date(item.modified_at).toLocaleString()}`
							: `Last edited ${new Date(item.modified_at).toLocaleString()}`
			: ''
	);

	let mode: EditorMode = $state('read');
	let saveStatus: EditorSaveStatus = $state('saved');

	$effect(() => {
		// Initialize mode based on module key when it changes
		untrack(() => {
			mode = key === 'notes' ? 'edit' : 'read';
		});
	});
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

	let isFolderBacked = $derived(item?.name === NOTE_MD_FILENAME);

	// Flush pending saves before navigation so events are dispatched while
	// components are still mounted, avoiding errors during destruction.
	beforeNavigate(() => {
		documentPage?.flush();
	});

	$effect(() => {
		if (item?.metadata?.attachments) {
			const serverAttachments = item.metadata.attachments.map((a: NoteAttachment) => {
				const isImage = a.mime_type?.startsWith('image/');
				// For folder-backed notes, use relative paths so markdown stays portable
				const path = isFolderBacked
					? `${ATTACHMENTS_FOLDER_NAME}/${a.name}`
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
			const newAttachments = [...serverAttachments, ...localOnly];

			untrack(() => {
				// Only update if actually different to prevent unnecessary downstream reactivity
				if (JSON.stringify(newAttachments) !== JSON.stringify(attachments)) {
					attachments = newAttachments;
				}
			});
		} else {
			untrack(() => {
				if (attachments.length > 0) {
					attachments = [];
				}
			});
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

	function getUpdateFunction(key: string, itemId: string) {
		return (data: { title: string; content: string }) => {
			switch (key) {
				case 'notes':
					return notesApi.update(itemId, { content: data.content, attachments: serializeNoteAttachments() });
				case 'decisions':
					return decisionsApi.update(itemId, { title: data.title, content: data.content });
				case 'meetings':
					return meetingsApi.update(itemId, { title: data.title, content: data.content });
				case 'standups':
					return standupsApi.update(itemId, { title: data.title, content: data.content });
				default:
					return Promise.reject(new Error(`Invalid module: ${key}`));
			}
		};
	}

	const saveMutation = createMutation<unknown, Error, { title: string; content: string }>({
		mutationFn: getUpdateFunction(key, id)
	});

	async function handleSave(event: CustomEvent<{ content: string; docId?: string }>) {
		if (event.detail.docId && event.detail.docId !== id) {
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
				queryClient.setQueryData(
					['module-item', key, id],
					(previous: ModuleItemQueryData | undefined) => {
						if (!previous) return previous;
						const modifiedAt = (saved as { modified_at?: string })?.modified_at ?? previous.modified_at;
						return {
							...previous,
							content: saveContent,
							current_version:
								(saved as { current_version?: number })?.current_version ?? previous.current_version,
							modified_at: modifiedAt,
							metadata: {
								...previous.metadata,
								attachments: noteAttachments,
								updated_at: modifiedAt
							}
						};
					}
				);
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

	function showErrorToast(err: unknown, fallbackMessage: string): void {
		const message = err instanceof Error ? err.message : fallbackMessage;
		console.error(fallbackMessage, err);
		toastStore.show(message, 'error');
	}

	async function withToastLoading<T>(
		setLoading: (value: boolean) => void,
		action: () => Promise<T>,
		options: {
			successMessage: string;
			errorMessage: string;
			onSuccess?: (result: T) => void;
		}
	): Promise<void> {
		setLoading(true);
		try {
			const result = await action();
			toastStore.show(options.successMessage, 'success');
			options.onSuccess?.(result);
		} catch (err) {
			showErrorToast(err, options.errorMessage);
		} finally {
			setLoading(false);
		}
	}

	async function resolveAttachmentFolder(item: ModuleItem): Promise<string | undefined> {
		if (!item.parent_folder_id) return undefined;
		if (!isFolderBacked) return item.parent_folder_id;

		try {
			const contents = await getFolderContents(item.parent_folder_id);
			const attachmentsFolder = contents.folders?.find((f: ApiFolder) => f.name === ATTACHMENTS_FOLDER_NAME);
			return attachmentsFolder?.id ?? item.parent_folder_id;
		} catch (err) {
			console.warn('Could not resolve attachments subfolder:', err);
			return item.parent_folder_id;
		}
	}

	async function uploadSingleFile(file: File, folderId: string): Promise<RichMarkdownAttachment> {
		let uploadName: string | undefined;
		if (isFolderBacked) {
			try {
				const folderContents = await getFolderContents(folderId);
				const existingNames = folderContents.files?.map((f: ApiFile) => f.name) || [];
				uploadName = generateUniqueFilename(file.name, existingNames);
			} catch (err) {
				console.warn('Could not check for filename collisions:', err);
			}
		}

		const uploaded = await uploadFile(folderId, file, undefined, uploadName);
		const isImage = uploaded.mime_type?.startsWith('image/');
		const relativePath = isFolderBacked ? `${ATTACHMENTS_FOLDER_NAME}/${uploaded.name}` : undefined;
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

		// Auto-insert relative link into editor for folder-backed notes
		if (isFolderBacked && documentPage) {
			documentPage.insertAttachment(attachment);
		}

		return attachment;
	}

	function handleUpload(event: CustomEvent<{ files: File[] }>) {
		void handleUploadAsync(event);
	}

	async function handleUploadAsync(event: CustomEvent<{ files: File[] }>) {
		if (!item?.parent_folder_id) {
			toastStore.show('This item must be saved to a folder before adding attachments', 'error');
			return;
		}

		const folderId = await resolveAttachmentFolder(item);
		const results = await Promise.allSettled(
			event.detail.files.map((file) => uploadSingleFile(file, folderId))
		);

		results.forEach((result, i) => {
			if (result.status === 'fulfilled') {
				attachments = [...attachments, result.value];
			} else {
				console.error('Failed to upload:', result.reason);
				toastStore.show(`Failed to upload ${event.detail.files[i].name}`, 'error');
			}
		});
	}

	async function handleDeleteAttachment(
		event: CustomEvent<{ attachment: RichMarkdownAttachment }>
	) {
		try {
			await deleteFile(event.detail.attachment.id);
			attachments = attachments.filter((a) => a.id !== event.detail.attachment.id);
		} catch (err) {
			showErrorToast(err, 'Failed to delete attachment');
		}
	}

	function handleSketch(event: CustomEvent<{ blob: Blob; filename: string }>) {
		void handleSketchAsync(event);
	}

	async function handleSketchAsync(event: CustomEvent<{ blob: Blob; filename: string }>) {
		if (!item || !item.parent_folder_id) return;

		if (isFolderBacked) {
			// Upload sketch PNG to drawings/ subfolder
			try {
				const contents = await getFolderContents(item.parent_folder_id);
				const drawingsFolder = contents.folders?.find(
					(f: ApiFolder) => f.name === DRAWINGS_FOLDER_NAME
				);
				if (drawingsFolder) {
					const folderContents = await getFolderContents(drawingsFolder.id);
					const existingNames = folderContents.files?.map((f: ApiFile) => f.name) || [];
					const uploadName = generateUniqueFilename(event.detail.filename, existingNames);
					const sketchFile = new File([event.detail.blob], uploadName, { type: 'image/png' });
					const uploaded = await uploadFile(drawingsFolder.id, sketchFile, undefined, uploadName);
					const attachment: RichMarkdownAttachment = {
						id: uploaded.id,
						filename: uploaded.name,
						path: `${DRAWINGS_FOLDER_NAME}/${uploaded.name}`,
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
				toastStore.show('Failed to save sketch', 'error');
				return;
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

	function handleShareNotification(event: {
		message: string;
		type: 'success' | 'error' | 'info';
	}) {
		toastStore.show(event.message, event.type);
	}

	function handleRenameConfirm(newTitle: string) {
		if (isRenaming) return;
		const trimmed = newTitle.trim();
		if (!trimmed) {
			renameError = 'Title is required';
			return;
		}
		renameError = '';
		void withToastLoading(
			(v) => isRenaming = v,
			async () => {
				if (key === 'notes') {
					await renameNote(id, { title: trimmed });
				} else if (key === 'decisions') {
					await decisionsApi.rename(id, { title: trimmed });
				}
			},
			{
				successMessage: key === 'notes' ? 'Note renamed' : 'Decision renamed',
				errorMessage: 'Failed to rename',
				onSuccess: () => {
					showRenameModal = false;
					renameError = '';
					$query.refetch();
				}
			}
		);
	}

	async function handleMoveConfirm(payload: { targetFolderId: string | null }) {
		if (isMoving || !item) return;
		await withToastLoading(
			(v) => {
				isMoving = v;
			},
			() => moveNote(id, { target_folder_id: payload.targetFolderId }),
			{
				successMessage: 'Note moved',
				errorMessage: 'Failed to move',
				onSuccess: () => {
					showMoveModal = false;
					$query.refetch();
				}
			}
		);
	}

	async function handleDuplicate() {
		if (isDuplicating || !item) return;
		await withToastLoading(
			(v) => {
				isDuplicating = v;
			},
			() => duplicateNote(id),
			{
				successMessage: 'Note duplicated',
				errorMessage: 'Failed to duplicate note',
				onSuccess: (duplicated) => goto(`/modules/notes/${(duplicated as { id: string }).id}`)
			}
		);
	}

	async function handleDeleteConfirm() {
		if (isDeleting || !item) return;
		await withToastLoading(
			(v) => {
				isDeleting = v;
			},
			() => deleteNote(id),
			{
				successMessage: 'Note deleted',
				errorMessage: 'Failed to delete',
				onSuccess: () => {
					showDeleteModal = false;
					goto(`/modules/${key}`);
				}
			}
		);
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
			subtitle={subtitle}
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
			initialAttachmentsOpen={initialAttachmentsOpen}
			on:save={handleSave}
			on:back={handleBack}
			on:modechange={handleModeChange}
			on:upload={handleUpload}
			on:delete={handleDeleteAttachment}
			on:sketch={handleSketch}
			on:rename={() => {
				showRenameModal = true;
				renameError = '';
			}}
			on:move={() => {
				showMoveModal = true;
			}}
			on:duplicate={handleDuplicate}
			on:deleteDocument={() => {
				showDeleteModal = true;
			}}
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
