<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import RichMarkdownEditor from '../../editor/components/RichMarkdownEditor.svelte';
	import type { EditorPermissions } from '../../editor/types';
	import { getFileContent, editFile } from '$lib/api/files';
	import { BaseEditor } from '$lib/components/editors';
	import type { File as ApiFile } from '$lib/api/types';

	let {
		open = false,
		file = null,
		onClose,
		onSaved
	}: {
		open?: boolean;
		file?: ApiFile | null;
		onClose?: () => void;
		onSaved?: (event: { file: ApiFile }) => void;
	} = $props();

	const dispatch = createEventDispatcher<{
		close: void;
		saved: { file: ApiFile };
	}>();

	let content = $state('');
	let currentMarkdown = $state('');
	let isLoading = $state(false);
	let isSaving = $state(false);
	let error = $state<string | null>(null);
	let saveMode: 'overwrite' | 'new_version' = $state('new_version');

	async function loadContent() {
		if (!file) return;
		const targetFileId = file.id;

		isLoading = true;
		error = null;

		try {
			const loadedContent = await getFileContent(targetFileId);
			if (file?.id !== targetFileId) return;
			content = loadedContent;
			currentMarkdown = loadedContent;
		} catch (err) {
			if (file?.id !== targetFileId) return;
			error = err instanceof Error ? err.message : 'Failed to load file content';
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(
		event: CustomEvent<{ saveMode: 'overwrite' | 'new_version'; changeDescription?: string }>
	) {
		if (!file || content === currentMarkdown) return;

		isSaving = true;
		error = null;

		try {
			const result = await editFile(
				file.id,
				currentMarkdown,
				event.detail.saveMode,
				event.detail.changeDescription
			);

			file = {
				...file,
				size: result.size,
				current_version: result.current_version,
				modified_at: result.modified_at
			};

			content = currentMarkdown;
			dispatch('saved', { file });
			onSaved?.({ file });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save file';
		} finally {
			isSaving = false;
		}
	}

	function handleClose() {
		dispatch('close');
		onClose?.();
	}

	$effect(() => {
		if (open && file) {
			const targetFileId = file.id;
			loadContent().then(() => {
				if (file?.id !== targetFileId) return;
				// content loaded for current file
			});
		}
	});

	const permissions: EditorPermissions = {
		canRead: true,
		canEdit: true,
		canUploadAttachments: true,
		canDeleteAttachments: true,
		canExport: true,
		canShare: true
	};
</script>

<BaseEditor
	{open}
	{file}
	{isLoading}
	{isSaving}
	{error}
	{saveMode}
	hasChanges={content !== currentMarkdown}
	title="Edit Markdown"
	on:close={handleClose}
	on:save={handleSave}
>
	<div class="flex h-full flex-col overflow-hidden bg-base-100">
		{#if !isLoading && file}
			<RichMarkdownEditor
				{content}
				editable={true}
				hasAttachmentHandler={false}
				bind:currentMarkdown
			/>
		{/if}
	</div>
</BaseEditor>
