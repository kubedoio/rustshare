<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { notesApi } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { meetingsApi } from '$lib/api/meetings';
	import { standupsApi } from '$lib/api/standups';
	import { getModuleByKey } from '$lib/modules/registry';
	import { goto } from '$app/navigation';
	import { Folder } from 'lucide-svelte';
	import MarkdownDocumentPage from '$lib/editor/components/MarkdownDocumentPage.svelte';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { EditorMode, EditorSaveStatus } from '$lib/editor/types';

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

	const query = createQuery<any, Error, any, any, string[]>({
		queryKey: ['module-item', key, id],
		queryFn: () => api?.get(id),
		enabled: !!api && !!id
	});

	let item = $derived($query.data);
	let content = $derived(item?.content ?? '');
	let title = $derived(item?.metadata?.title || item?.name || '');
	let modifiedAt = $derived(
		item?.modified_at
			? key === 'meetings' && item?.metadata?.date
				? `Date: ${new Date(item.metadata.date).toLocaleDateString()}${item.metadata.attendees?.length ? ` • ${item.metadata.attendees.length} attendee${item.metadata.attendees.length === 1 ? '' : 's'}` : ''} • Last edited ${new Date(item.modified_at).toLocaleString()}`
				: `Last edited ${new Date(item.modified_at).toLocaleString()}`
			: ''
	);
	let mode: EditorMode = $state('read');
	let saveStatus: EditorSaveStatus = $state('saved');

	let breadcrumb = $derived([
		{ label: module?.displayName || key, onClick: () => goto(`/modules/${key}`) },
		{ label: title }
	]);

	const saveMutation = createMutation<any, Error, { title: string; content: string }>({
		mutationFn: (data: { title: string; content: string }) => {
			if (key === 'notes') return notesApi.update(id, { content: data.content });
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

	async function handleOpenInFiles() {
		if (module?.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
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
			<button class="btn mt-4 btn-ghost" on:click={() => $query.refetch()}>Retry</button>
		</div>
	{:else if item}
		<MarkdownDocumentPage
			{title}
			{content}
			{mode}
			{saveStatus}
			{breadcrumb}
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
		>
			<button
				slot="extraActions"
				class="btn btn-ghost btn-sm gap-1.5"
				on:click={handleOpenInFiles}
			>
				<Folder size={14} />
				<span>Open in Files</span>
			</button>
		</MarkdownDocumentPage>
	{/if}
</div>

<style>
	.module-detail-page {
		background: var(--rs-bg);
		height: 100%;
	}
</style>
