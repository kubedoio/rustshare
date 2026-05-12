<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { notesApi } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { meetingsApi } from '$lib/api/meetings';
	import { standupsApi } from '$lib/api/standups';
	import { uploadFile, deleteFile } from '$lib/api/files';
	import { getModuleByKey } from '$lib/modules/registry';
	import { goto } from '$app/navigation';
	import { Folder, Share2, Pencil } from 'lucide-svelte';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import ShareModal from '$lib/components/modals/ShareModal.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import { toastStore } from '$lib/stores/toast';
	import type { EditorMode, EditorSaveStatus, RichMarkdownAttachment } from '$lib/editor/types';
	import { classifyAttachmentKind } from '$lib/editor/validation';

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
	let mode: EditorMode = $state('read');
	let saveStatus: EditorSaveStatus = $state('saved');
	let showShareModal = $state(false);
	let showRenameModal = $state(false);
	let renameError = $state('');
	let isRenaming = $state(false);
	let attachments = $state<RichMarkdownAttachment[]>([]);

	$effect(() => {
		if (item?.metadata?.attachments) {
			attachments = item.metadata.attachments.map((a: any) => ({
				id: a.file_id,
				filename: a.name,
				path: a.mime_type?.startsWith('image/')
					? `/api/v1/files/${a.file_id}/preview`
					: `/api/v1/files/${a.file_id}/content`,
				mimeType: a.mime_type,
				size: a.size,
				kind: classifyAttachmentKind(a.mime_type),
				createdAt: a.created_at,
				createdBy: ''
			}));
		} else {
			attachments = [];
		}
	});

	let breadcrumb = $derived([
		{ label: module?.displayName || key, onClick: () => goto(`/modules/${key}`) },
		{ label: title }
	]);

	const saveMutation = createMutation<any, Error, { title: string; content: string }>({
		mutationFn: (data: { title: string; content: string }) => {
			const noteAttachments = attachments.map((a) => ({
				file_id: a.id,
				name: a.filename,
				mime_type: a.mimeType,
				size: a.size,
				created_at: a.createdAt
			}));
			if (key === 'notes') return notesApi.update(id, { content: data.content, attachments: noteAttachments });
			if (key === 'decisions')
				return decisionsApi.update(id, { title: data.title, content: data.content });
			if (key === 'meetings')
				return meetingsApi.update(id, { title: data.title, content: data.content });
			if (key === 'standups')
				return standupsApi.update(id, { title: data.title, content: data.content });
			return Promise.reject('Invalid module');
		},
		onSuccess: () => {
			saveStatus = 'saved';
			$query.refetch();
		},
		onError: () => {
			saveStatus = 'error';
		}
	});

	async function handleSave(event: CustomEvent<{ content: string }>) {
		saveStatus = 'saving';
		await $saveMutation.mutateAsync({ title, content: event.detail.content });
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
		for (const file of event.detail.files) {
			try {
				const uploaded = await uploadFile(item.parent_folder_id, file);
				const isImage = uploaded.mime_type?.startsWith('image/');
				const attachment: RichMarkdownAttachment = {
					id: uploaded.id,
					filename: uploaded.name,
					path: isImage
						? `/api/v1/files/${uploaded.id}/preview`
						: `/api/v1/files/${uploaded.id}/content`,
					mimeType: uploaded.mime_type,
					size: uploaded.size,
					kind: classifyAttachmentKind(uploaded.mime_type),
					createdAt: uploaded.created_at,
					createdBy: ''
				};
				attachments = [...attachments, attachment];
			} catch (err) {
				console.error('Failed to upload attachment:', err);
				toastStore.show('Failed to upload attachment', 'error');
			}
		}
	}

	async function handleDeleteAttachment(event: CustomEvent<{ attachment: RichMarkdownAttachment }>) {
		try {
			await deleteFile(event.detail.attachment.id);
			attachments = attachments.filter((a) => a.id !== event.detail.attachment.id);
		} catch (err) {
			console.error('Failed to delete attachment:', err);
			toastStore.show('Failed to delete attachment', 'error');
		}
	}

	function handleSketch(event: CustomEvent<{ blob: Blob; filename: string }>) {
		// Sketches are base64-embedded by the editor internally.
		// This handler is a no-op for module documents.
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
			await decisionsApi.rename(id, { title: trimmed });
			showRenameModal = false;
			renameError = '';
			$query.refetch();
			toastStore.show('Decision renamed', 'success');
		} catch (err) {
			console.error('Failed to rename decision:', err);
			renameError = err instanceof Error ? err.message : 'Failed to rename decision';
		} finally {
			isRenaming = false;
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
			{title}
			{content}
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
			on:save={handleSave}
			on:back={handleBack}
			on:modechange={handleModeChange}
			on:upload={handleUpload}
			on:delete={handleDeleteAttachment}
			on:sketch={handleSketch}
		>
			<svelte:fragment slot="extraActions">
				{#if key === 'notes'}
					<button
						class="btn btn-ghost btn-sm gap-1.5"
						onclick={() => (showShareModal = true)}
					>
						<Share2 size={14} />
						<span>Share</span>
					</button>
				{/if}

				{#if key === 'decisions'}
					<button
						class="btn btn-ghost btn-sm gap-1.5"
						onclick={() => { showRenameModal = true; renameError = ''; }}
					>
						<Pencil size={14} />
						<span>Rename</span>
					</button>
				{/if}

				<button
					class="btn btn-ghost btn-sm gap-1.5"
					onclick={handleOpenInFiles}
				>
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

		{#if key === 'decisions'}
			<PromptModal
				open={showRenameModal}
				title="Rename decision"
				message="New title"
				defaultValue={title}
				confirmLabel="Rename"
				error={renameError}
				isLoading={isRenaming}
				onConfirm={handleRenameConfirm}
				onCancel={() => { showRenameModal = false; renameError = ''; }}
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
