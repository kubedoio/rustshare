<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation } from '$lib/query-compat';
	import { notesApi } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { meetingsApi } from '$lib/api/meetings';
	import { getModuleByKey } from '$lib/modules/registry';
	import { ArrowLeft, Save, Trash2, ExternalLink } from 'lucide-svelte';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

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
			$query.refetch();
		}
	});

	async function handleSave() {
		await $saveMutation.mutateAsync({ title, content });
	}

	function handleBack() {
		goto(`/modules/${key}`);
	}
</script>

<div class="detail-page-container">
	<header class="detail-header rs-surface">
		<div class="header-left">
			<button class="btn btn-ghost btn-sm btn-square" on:click={handleBack}>
				<ArrowLeft size={20} />
			</button>
			<div class="title-block">
				<input 
					type="text" 
					bind:value={title} 
					class="title-input" 
					placeholder="Enter title..."
				/>
				{#if item}
					<span class="path-info">{item.path}</span>
				{/if}
			</div>
		</div>

		<div class="header-right">
			<button class="btn btn-primary btn-sm gap-2" on:click={handleSave} disabled={$saveMutation.isPending}>
				<Save size={14} />
				<span>{$saveMutation.isPending ? 'Saving...' : 'Save Changes'}</span>
			</button>
		</div>
	</header>

	<main class="detail-content rs-surface">
		{#if $query.isLoading}
			<div class="flex h-64 items-center justify-center">
				<div class="loading loading-lg loading-spinner text-brand-500"></div>
			</div>
		{:else if $query.error}
			<div class="p-8 text-center">
				<p class="text-error">Failed to load item.</p>
				<button class="btn btn-ghost mt-4" on:click={() => $query.refetch()}>Retry</button>
			</div>
		{:else if item}
			<textarea 
				bind:value={content} 
				class="content-editor"
				placeholder="Write your content here (markdown supported)..."
			></textarea>
		{/if}
	</main>
</div>

<style>
	.detail-page-container {
		max-width: 1200px;
		margin: 0 auto;
		padding: 0 2rem 2rem;
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		height: calc(100vh - 120px);
	}

	.detail-header {
		padding: 1rem 1.5rem;
		border-radius: var(--rs-radius-lg);
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 2rem;
	}

	.header-left {
		display: flex;
		align-items: center;
		gap: 1rem;
		flex: 1;
		min-width: 0;
	}

	.title-block {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.title-input {
		background: transparent;
		border: none;
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--rs-text);
		padding: 0;
		outline: none;
		width: 100%;
	}

	.path-info {
		font-size: 0.75rem;
		color: var(--rs-text-muted);
		font-family: monospace;
	}

	.detail-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		border-radius: var(--rs-radius-lg);
		overflow: hidden;
	}

	.content-editor {
		flex: 1;
		background: transparent;
		border: none;
		resize: none;
		padding: 2rem;
		font-family: var(--rs-font-mono, monospace);
		font-size: 1rem;
		line-height: 1.6;
		color: var(--rs-text);
		outline: none;
	}

	.header-right {
		display: flex;
		gap: 0.5rem;
	}
</style>
