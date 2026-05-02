<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { Folder, FileText, Plus } from 'lucide-svelte';

	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	const folderContentsQuery = createQuery({
		queryKey: ['module-folder-contents', module.key],
		queryFn: () => getModuleRootContents(module.rootPath)
	});

	$: contents = $folderContentsQuery.data;

	async function handleCreateFromTemplate() {
		if (!module.defaultTemplate) {
			alert('No default template configured for this module.');
			return;
		}
		const name = window.prompt('Enter a name for the new item:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: module.defaultTemplate,
				name,
				parent_folder_id: null
			});
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
		} catch (err) {
			console.error('Failed to create from template:', err);
			alert(err instanceof Error ? err.message : 'Failed to create item');
		}
	}

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No items yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ??
		'Create your first item from a template to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'Create from Template';

	$: isGallery = module.ui.page.layout === 'gallery-grid';
</script>

<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6">
	<div class="mb-4 flex items-center justify-between">
		<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Contents</h2>
		<button class="btn btn-sm btn-primary" onclick={handleCreateFromTemplate}>
			<Plus size={14} />
			<span>Create from Template</span>
		</button>
	</div>

	{#if $folderContentsQuery.isLoading}
		<div class="flex h-32 items-center justify-center">
			<div class="loading loading-md loading-spinner text-brand-500"></div>
		</div>
	{:else if !contents || (contents.folders?.length === 0 && contents.files?.length === 0)}
		<EmptyState
			icon={Folder}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateFromTemplate}
		/>
	{:else}
		{#if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each contents.folders ?? [] as folder}
					<a
						href="/files?folder={folder.id}"
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-info/10 text-info">
							<Folder size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{folder.name}</span>
							<span class="text-xs text-base-content/40">{folder.path}</span>
						</div>
					</a>
				{/each}

				{#each contents.files ?? [] as file}
					<a
						href="/files?preview={file.id}"
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{file.name}</span>
							<span class="text-xs text-base-content/40">{file.path}</span>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-2">
				{#if contents.folders?.length > 0}
					<div class="mb-2 text-xs font-semibold tracking-wider text-base-content/40 uppercase">
						Folders
					</div>
					{#each contents.folders as folder}
						<a
							href="/files?folder={folder.id}"
							class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
						>
							<div class="flex h-9 w-9 items-center justify-center rounded-lg bg-info/10 text-info">
								<Folder size={16} />
							</div>
							<div class="flex flex-col">
								<span class="text-sm font-medium text-base-content">{folder.name}</span>
								<span class="text-xs text-base-content/40">{folder.path}</span>
							</div>
						</a>
					{/each}
				{/if}

				{#if contents.files?.length > 0}
					<div class="mt-4 mb-2 text-xs font-semibold tracking-wider text-base-content/40 uppercase">
						Files
					</div>
					{#each contents.files as file}
						<a
							href="/files?preview={file.id}"
							class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
						>
							<div
								class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
							>
								<FileText size={16} />
							</div>
							<div class="flex flex-col">
								<span class="text-sm font-medium text-base-content">{file.name}</span>
								<span class="text-xs text-base-content/40">{file.path}</span>
							</div>
						</a>
					{/each}
				{/if}
			</div>
		{/if}
	{/if}
</div>

