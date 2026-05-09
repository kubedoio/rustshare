<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import { FileText, Plus, Clock, Folder } from 'lucide-svelte';

	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: isGallery = module.ui.page.layout === 'gallery-grid';

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No standup records yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Create your first standup to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Standup';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['standups-root', module.key],
		queryFn: () => getModuleRootContents(module.rootPath),
		enabled: true
	});

	$: contents = $rootFolderQuery.data;
	$: standups = filterUserVisibleEntries(contents?.files ?? []);

	let showPromptModal = false;
	let createError = '';
	let showDuplicateConfirm = false;
	let pendingName = '';

	async function handleCreateStandupConfirm(name: string) {
		const trimmed = name.trim();
		if (!trimmed) return;
		if (!module.defaultTemplate) return;

		createError = '';

		const exists = standups.some((s) => s.name?.toLowerCase() === trimmed.toLowerCase());
		if (exists) {
			pendingName = trimmed;
			showDuplicateConfirm = true;
			return;
		}

		await doCreateStandup(trimmed);
	}

	async function doCreateStandup(name: string) {
		const template = module.defaultTemplate;
		if (!template) return;
		try {
			const result = await createFromTemplate({
				template_key: template,
				name,
				parent_folder_id: null
			});
			showPromptModal = false;
			createError = '';
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create standup:', err);
			createError = err instanceof Error ? err.message : 'Failed to create standup';
		}
	}

	async function handleDuplicateProceed() {
		showDuplicateConfirm = false;
		await doCreateStandup(pendingName);
	}

	function handleCreateStandup() {
		if (!module.defaultTemplate) return;
		showPromptModal = true;
		createError = '';
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function navigateToStandup(fileId: string) {
		goto(getModuleObjectHref(module.key, 'file', fileId));
	}
</script>

<ModulePageShell title="Standups" subtitle={module.description}>
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleCreateStandup}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New Standup</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if $rootFolderQuery.isLoading}
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if standups.length === 0 && contents?.folders?.length === 0}
			<EmptyState
				icon={"📊"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateStandup}
			/>
		{:else if standups.length > 0}
			{#if isGallery}
				<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
					{#each standups as standup}
						<button
							class="group flex flex-col gap-3 rounded-xl border border-base-300/40 bg-base-100 p-4 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
							onclick={() => navigateToStandup(standup.id)}
						>
							<div
								class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
							>
								<FileText size={18} />
							</div>
							<div class="flex flex-col">
								<span class="text-sm font-medium text-base-content">{standup.name}</span>
								<span class="flex items-center gap-1 text-xs text-base-content/40">
									<Clock size={12} />
									{new Date(standup.modified_at).toLocaleDateString()}
								</span>
							</div>
						</button>
					{/each}
				</div>
			{:else}
				<div class="flex flex-col gap-3">
					{#each standups as standup}
						<button
							class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
							onclick={() => navigateToStandup(standup.id)}
						>
							<div
								class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
							>
								<FileText size={18} />
							</div>
							<div class="flex min-w-0 flex-col gap-1">
								<span class="truncate text-sm font-medium text-base-content">{standup.name}</span>
								<div class="flex items-center gap-3 text-xs text-base-content/50">
									<span class="inline-flex items-center gap-1">
										<Clock size={12} />
										{new Date(standup.modified_at).toLocaleDateString()}
									</span>
								</div>
							</div>
						</button>
					{/each}
				</div>
			{/if}
		{:else}
			<p class="text-sm text-base-content/50">
				No standups yet. Create your first standup record to get started.
			</p>
		{/if}
	</div>
</ModulePageShell>

<PromptModal
	open={showPromptModal}
	title="New Standup"
	message="Enter a name for the new standup record:"
	confirmLabel="Create"
	error={createError}
	onConfirm={handleCreateStandupConfirm}
	onCancel={() => {
		showPromptModal = false;
		createError = '';
	}}
/>

<ConfirmModal
	open={showDuplicateConfirm}
	title="Duplicate Name"
	message={`A standup named "${pendingName}" already exists. Create anyway?`}
	confirmLabel="Create Anyway"
	onConfirm={handleDuplicateProceed}
	onCancel={() => {
		showDuplicateConfirm = false;
		pendingName = '';
	}}
/>
