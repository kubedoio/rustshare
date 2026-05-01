<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { notesApi } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { meetingsApi } from '$lib/api/meetings';
	import { getModuleByKey } from '$lib/modules/registry';
	import { goto } from '$app/navigation';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import type { EditorMode, EditorSaveStatus } from '$lib/editor/types';

	$: key = ($page.params.key || '') as string;
	$: id = ($page.params.id || '') as string;
	$: module = getModuleByKey(key);

	// Determine which API to use
	$: api = key === 'notes' ? notesApi : 
	         key === 'decisions' ? decisionsApi : 
			 key === 'meetings' ? meetingsApi : null;

	$: query = createQuery<any, Error, any, any, string[]>({
		queryKey: ['module-item', key, id],
		queryFn: () => api?.get(id),
		enabled: !!api && !!id
	});

	$: item = $query.data;
	let content = '';
	let title = '';
	let mode: EditorMode = 'read';
	let saveStatus: EditorSaveStatus = 'saved';

	$: if (item) {
		content = item.content;
		title = item.metadata?.title || item.name;
	}

	const saveMutation = createMutation<any, Error, { title: string; content: string }>({
		mutationFn: (data: { title: string; content: string }) => {
			if (key === 'notes') return notesApi.update(id, { content: data.content });
			if (key === 'decisions') return decisionsApi.update(id, { title: data.title, content: data.content });
			if (key === 'meetings') return meetingsApi.update(id, { title: data.title, content: data.content });
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
</script>

<div class="module-detail-page h-full">
	{#if $query.isLoading}
		<div class="flex h-full items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else if $query.error}
		<div class="p-8 text-center h-full flex flex-col items-center justify-center">
			<p class="text-error">Failed to load item.</p>
			<button class="btn btn-ghost mt-4" on:click={() => $query.refetch()}>Retry</button>
		</div>
	{:else if item}
		<MarkdownDocumentPage
			{title}
			{content}
			{mode}
			{saveStatus}
			label={module?.displayName || key}
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
	{/if}
</div>

<style>
	.module-detail-page {
		background: var(--rs-bg);
		height: 100%;
	}
</style>
