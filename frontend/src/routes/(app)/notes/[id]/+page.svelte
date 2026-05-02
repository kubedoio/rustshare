<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { getNote, saveNote, renameNote, deleteNote, toggleVisibility } from '$lib/api/notes';
	import { uploadFile } from '$lib/api/files';

	function extractH1(md: string): string | null {
		const match = md.match(/^#\s+(.+)$/m);
		return match ? match[1].trim() : null;
	}
	import type { Note } from '$lib/api/types';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import type { EditorMode, EditorSaveStatus } from '$lib/editor/types';

	const noteId = $page.params.id as string;

	let note: Note | null = null;
	let title = '';
	let content = '';
	let color: string | null = null;
	let isLoading = true;
	let saveStatus: EditorSaveStatus = 'saved';
	let mode: EditorMode = 'read';
	let editorPage: MarkdownDocumentPage;

	onMount(() => {
		loadNote();
	});

	async function loadNote() {
		isLoading = true;
		try {
			note = await getNote(noteId);
			title = note.metadata.title;
			content = note.content;
			color = note.metadata.color || null;
		} catch (err) {
			console.error('Failed to load note', err);
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(event: CustomEvent<{ content: string; color?: string | null }>) {
		if (!note) return;
		saveStatus = 'saving';
		try {
			const { content: newContent, color: newColor } = event.detail;
			await saveNote(noteId, { content: newContent, color: newColor });
			content = newContent;
			if (newColor !== undefined) color = newColor;

			const newH1 = extractH1(content);
			if (newH1 && newH1 !== note.metadata.title) {
				const renamed = await renameNote(noteId, { title: newH1 });
				note = renamed;
				title = renamed.metadata.title;
			}

			saveStatus = 'saved';
		} catch (err) {
			saveStatus = 'error';
		}
	}

	async function handleSketch(event: CustomEvent<{ blob: Blob; filename: string }>) {
		if (!note) return;

		try {
			const { blob, filename } = event.detail;
			const file = new File([blob], filename, { type: 'image/png' });

			// Upload to the same folder as the note
			const uploadedFile = await uploadFile(note.parent_folder_id, file);

			// Insert image into markdown
			// We can use the attachment insert logic if we wrap it
			const attachment = {
				id: uploadedFile.id,
				filename: uploadedFile.name,
				mimeType: uploadedFile.mime_type,
				size: uploadedFile.size,
				url: `/api/v1/files/${uploadedFile.id}/preview`,
				kind: 'image' as const,
				createdAt: uploadedFile.created_at
			};

			if (editorPage) {
				editorPage.insertAttachment(attachment);
			}
		} catch (err) {
			console.error('Failed to upload sketch', err);
			alert('Failed to upload sketch');
		}
	}

	function handleBack() {
		goto('/notes');
	}

	function handleModeChange(event: CustomEvent<{ mode: EditorMode }>) {
		mode = event.detail.mode;
	}
</script>

<div class="note-page h-full">
	{#if isLoading}
		<div class="flex h-full items-center justify-center">
			<div class="loading loading-lg loading-spinner"></div>
		</div>
	{:else if note}
		<MarkdownDocumentPage
			bind:this={editorPage}
			{title}
			{content}
			{color}
			{mode}
			{saveStatus}
			label="Notes"
			permissions={{
				canRead: true,
				canEdit: true,
				canUploadAttachments: true,
				canDeleteAttachments: true,
				canExport: true,
				canShare: true
			}}
			on:save={handleSave}
			on:sketch={handleSketch}
			on:back={handleBack}
			on:modechange={handleModeChange}
		/>
	{:else}
		<div class="flex h-full flex-col items-center justify-center p-8 text-center">
			<p class="text-error">Note not found</p>
			<button class="btn mt-4 btn-ghost" on:click={handleBack}>Back to Notes</button>
		</div>
	{/if}
</div>

<style>
	.note-page {
		background: var(--rs-bg);
		height: 100%;
	}
</style>
