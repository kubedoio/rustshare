<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { Folder, FileText, Plus } from 'lucide-svelte';

	export let moduleConfig: {
		module_key: string;
		display_name: string;
		description: string;
		icon: string;
		root_path: string;
		default_template: string | null;
		ui_config?: {
			modulePage?: {
				emptyStateTitle?: string;
				emptyStateDescription?: string;
				emptyStateAction?: string;
			};
		};
	};

	const folderContentsQuery = createQuery({
		queryKey: ['module-folder-contents', moduleConfig.module_key],
		queryFn: async () => {
			const res = await fetch(`/api/v1/folders/root/contents`);
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			return { ...contents, current_folder: folder };
		}
	});

	$: contents = $folderContentsQuery.data;

	async function handleCreateFromTemplate() {
		if (!moduleConfig.default_template) {
			alert('No default template configured for this module.');
			return;
		}
		const name = window.prompt('Enter a name for the new item:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			if (result.object_type === 'folder') {
				goto(`/files?folder=${result.object_id}`);
			} else {
				goto(`/files?preview=${result.object_id}`);
			}
		} catch (err) {
			console.error('Failed to create from template:', err);
			alert(err instanceof Error ? err.message : 'Failed to create item');
		}
	}

	$: emptyTitle = moduleConfig.ui_config?.modulePage?.emptyStateTitle ?? 'No items yet';
	$: emptyDescription =
		moduleConfig.ui_config?.modulePage?.emptyStateDescription ??
		'Create your first item from a template to get started.';
	$: emptyAction = moduleConfig.ui_config?.modulePage?.emptyStateAction ?? 'Create from Template';
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
</div>
