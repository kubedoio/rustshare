<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { Folder, Plus, ArrowRight } from 'lucide-svelte';

	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No shares yet';
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
	$: sharePackages = contents?.folders ?? [];

	async function handleCreateShare() {
		if (!module.defaultTemplate) return;
		const name = window.prompt('Enter a name for the new share package:');
		if (!name) return;
		try {
			await createFromTemplate({
				template_key: module.defaultTemplate,
				name,
				parent_folder_id: null
			});
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create share:', err);
		}
	}

	function handleOpenInFiles() {
		if (module.rootPath) {
			goto(`/files?path=${encodeURIComponent(module.rootPath)}`);
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
		{#if sharePackages.length === 0 && contents?.files?.length === 0}
			<EmptyState
				icon={Folder}
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
