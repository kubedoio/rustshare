<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import RichMarkdownEditor from '../../editor/components/RichMarkdownEditor.svelte';
	import type { EditorPermissions } from '../../editor/types';
	import { getFileContent, editFile } from '$lib/api/files';
	import { BaseEditor } from '$lib/components/editors';
	import type { File as ApiFile } from '$lib/api/types';

	export let open = false;
	export let file: ApiFile | null = null;

	const dispatch = createEventDispatcher<{
		close: void;
		saved: { file: ApiFile };
	}>();

	let content = '';
	let currentMarkdown = '';
	let isLoading = false;
	let isSaving = false;
	let error: string | null = null;
	let saveMode: 'overwrite' | 'new_version' = 'new_version';

	async function loadContent() {
		if (!file) return;

		isLoading = true;
		error = null;

		try {
			const loadedContent = await getFileContent(file.id);
			content = loadedContent;
			currentMarkdown = loadedContent;
		} catch (err) {
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
		} catch (err) {
			error = err instanceof Error ? err.message : 'Failed to save file';
		} finally {
			isSaving = false;
		}
	}

	function handleClose() {
		dispatch('close');
	}

	$: if (open && file) {
		loadContent();
	}

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
			<RichMarkdownEditor {content} editable={true} hasAttachmentHandler={true} bind:currentMarkdown />
		{/if}
	</div>
</BaseEditor>
