<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import { activityStore, type ActivityType } from '$lib/stores/activity';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { Folder, FileText, Plus } from 'lucide-svelte';

	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import type { ModuleDefinition } from '$lib/modules/registry';

	interface Props {
		module: ModuleDefinition;
	}

	let { module }: Props = $props();

	const folderContentsQuery = createQuery({
		queryKey: ['module-folder-contents', module.key],
		queryFn: () => getModuleRootContents(module.rootPath)
	});

	let contents = $derived($folderContentsQuery.data);
	let visibleFolders = $derived(filterUserVisibleEntries(contents?.folders ?? []));
	let visibleFiles = $derived(filterUserVisibleEntries(contents?.files ?? []));

	let showPromptModal = $state(false);
	let templateError = $state('');

	function getActivityTypeForModule(key: string): ActivityType | null {
		switch (key) {
			case 'meetings':
				return 'meeting_created';
			case 'standups':
				return 'standup_created';
			case 'kanban':
				return 'kanban_created';
			case 'decisions':
				return 'decision_created';
			case 'brainstorming':
				return 'brainstorm_created';
			case 'notes':
				return 'note_created';
			default:
				return null;
		}
	}

	async function handleCreateFromTemplateConfirm(name: string) {
		showPromptModal = false;
		if (!module.defaultTemplate) return;
		try {
			const result = await createFromTemplate({
				template_key: module.defaultTemplate,
				name,
				parent_folder_id: null
			});
			const activityType = getActivityTypeForModule(module.key);
			if (activityType) {
				activityStore.addActivity(activityType, name || 'Untitled', {
					artifactId: result.object_id,
					moduleKey: module.key
				});
			}
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
		} catch (err) {
			console.error('Failed to create from template:', err);
		}
	}

	function handleCreateFromTemplate() {
		if (!module.defaultTemplate) {
			templateError = 'No default template configured for this module.';
			return;
		}
		templateError = '';
		showPromptModal = true;
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No items yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ?? 'Create your first item from a template to get started.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'Create from Template');

	let isGallery = $derived(module.ui.page.layout === 'gallery-grid');
</script>

<ModulePageShell title={module.displayName} subtitle={module.description}>
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleCreateFromTemplate}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>Create from Template</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if templateError}
			<div class="rounded-lg border border-error/30 bg-error/10 px-4 py-2 text-sm text-error">
				{templateError}
			</div>
		{/if}
		{#if $folderContentsQuery.isLoading}
			<ModulePageSkeleton />
		{:else if $folderContentsQuery.isError}
			<ErrorState
				title="Failed to load folder contents"
				message={$folderContentsQuery.error?.message || 'Unknown error'}
				onRetry={() => $folderContentsQuery.refetch()}
			/>
		{:else if visibleFolders.length === 0 && visibleFiles.length === 0}
			<EmptyState
				icon={Folder}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateFromTemplate}
			/>
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each visibleFolders as folder}
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

				{#each visibleFiles as file}
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
				{#if visibleFolders.length > 0}
					<div class="mb-2 text-xs font-semibold tracking-wider text-base-content/40 uppercase">
						Folders
					</div>
					{#each visibleFolders as folder}
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

				{#if visibleFiles.length > 0}
					<div
						class="mt-4 mb-2 text-xs font-semibold tracking-wider text-base-content/40 uppercase"
					>
						Files
					</div>
					{#each visibleFiles as file}
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
</ModulePageShell>

<PromptModal
	open={showPromptModal}
	title="Create from Template"
	message="Enter a name for the new item:"
	confirmLabel="Create"
	onConfirm={handleCreateFromTemplateConfirm}
	onCancel={() => (showPromptModal = false)}
/>
