<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import {
		getNote,
		saveNote,
		renameNote,
		deleteNote,
		toggleVisibility
	} from '$lib/api/notes';
	import type { Note } from '$lib/api/types';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import type { EditorMode, EditorSaveStatus } from '$lib/editor/types';

	const noteId = $page.params.id as string;

	let note: Note | null = null;
	let title = '';
	let content = '';
	let isLoading = true;
	let saveStatus: EditorSaveStatus = 'saved';
	let mode: EditorMode = 'read';

	onMount(() => {
		loadNote();
	});

	async function loadNote() {
		isLoading = true;
		try {
			note = await getNote(noteId);
			title = note.metadata.title;
			content = note.content;
		} catch (err) {
			console.error('Failed to load note', err);
		} finally {
			isLoading = false;
		}
	}

	async function handleSave(event: CustomEvent<{ content: string }>) {
		if (!note) return;
		saveStatus = 'saving';
		try {
			await saveNote(noteId, event.detail.content);
			content = event.detail.content;
			saveStatus = 'saved';
		} catch (err) {
			saveStatus = 'error';
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
			{title}
			{content}
			{mode}
			{saveStatus}
			label="Notes"
			permissions={{
				canEdit: true,
				canUploadAttachments: true,
				canDeleteAttachments: true,
				canExport: true
			}}
			on:save={handleSave}
			on:back={handleBack}
			on:modechange={handleModeChange}
		/>
	{:else}
		<div class="p-8 text-center h-full flex flex-col items-center justify-center">
			<p class="text-error">Note not found</p>
			<button class="btn btn-ghost mt-4" on:click={handleBack}>Back to Notes</button>
		</div>
	{/if}
</div>

<style>
	.note-page {
		background: var(--rs-bg);
		height: 100%;
	}
</style>
