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
	import { Folder, Plus, ArrowRight } from 'lucide-svelte';

	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import { toastStore } from '$lib/stores/toast';

	export let module: ModuleDefinition;

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No share packages yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Create your first share package to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Share';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['shares-root', module.key],
		queryFn: () => getModuleRootContents(module.rootPath),
		enabled: true
	});

	$: contents = $rootFolderQuery.data;
	$: sharePackages = filterUserVisibleEntries(contents?.folders ?? []);

	let showPromptModal = false;
	let createError = '';
	let showDuplicateConfirm = false;
	let pendingName = '';

	async function handleCreateShareConfirm(name: string) {
		const trimmed = name.trim();
		if (!trimmed) return;
		if (!module.defaultTemplate) return;

		createError = '';

		const exists = sharePackages.some((p) => p.name?.toLowerCase() === trimmed.toLowerCase());
		if (exists) {
			pendingName = trimmed;
			showDuplicateConfirm = true;
			return;
		}

		await doCreateShare(trimmed);
	}

	async function doCreateShare(name: string) {
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
			toastStore.show(
				'Share package created. Add files and folders to share.',
				'success',
				5000
			);
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create share:', err);
			createError = err instanceof Error ? err.message : 'Failed to create share';
		}
	}

	async function handleDuplicateProceed() {
		showDuplicateConfirm = false;
		await doCreateShare(pendingName);
	}

	function handleCreateShare() {
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

	function navigateToShare(folderId: string) {
		goto(getModuleObjectHref(module.key, 'folder', folderId));
	}
</script>

<ModulePageShell title="Shares" subtitle={module.description}>
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleCreateShare}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New Share</span>
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
		{:else if sharePackages.length === 0 && contents?.files?.length === 0}
			<EmptyState
				icon={"🔗"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateShare}
			/>
		{:else if sharePackages.length > 0}
			<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
				{#each sharePackages as pkg}
					<button
						class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						onclick={() => navigateToShare(pkg.id)}
					>
						<div class="flex items-start justify-between">
							<div class="flex items-center gap-2">
								<Folder size={18} class="text-brand-500" />
								<span class="text-sm font-medium text-base-content">{pkg.name}</span>
							</div>
							<ArrowRight
								size={14}
								class="text-base-content/30 transition-transform group-hover:translate-x-0.5"
							/>
						</div>
						<div class="flex items-center gap-2 text-xs text-base-content/50">
							<span>Updated {new Date(pkg.updated_at).toLocaleDateString()}</span>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">
				No share packages yet. Create your first share package to get started.
			</p>
		{/if}
	</div>
</ModulePageShell>

<PromptModal
	open={showPromptModal}
	title="New Share Package"
	message="Enter a name for the new share package:"
	confirmLabel="Create"
	error={createError}
	onConfirm={handleCreateShareConfirm}
	onCancel={() => {
		showPromptModal = false;
		createError = '';
	}}
/>

<ConfirmModal
	open={showDuplicateConfirm}
	title="Duplicate Name"
	message={`A share package named "${pendingName}" already exists. Create anyway?`}
	confirmLabel="Create Anyway"
	onConfirm={handleDuplicateProceed}
	onCancel={() => {
		showDuplicateConfirm = false;
		pendingName = '';
	}}
/>
