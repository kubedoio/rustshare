<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { FileText, Plus, Clock, Folder } from 'lucide-svelte';

	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: isGallery = module.ui.page.layout === 'gallery-grid';

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No standups yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Create your first standup record to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Standup';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['standups-root', module.key],
		queryFn: () => getModuleRootContents(module.rootPath),
		enabled: true
	});

	$: contents = $rootFolderQuery.data;
	$: standups = contents?.files ?? [];

	async function handleCreateStandup() {
		if (!module.defaultTemplate) return;
		const name = window.prompt('Enter a name for the new standup record:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: module.defaultTemplate,
				name,
				parent_folder_id: null
			});
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create standup:', err);
		}
	}

	function handleOpenInFiles() {
		if (module.rootPath) {
			goto(`/files?path=${encodeURIComponent(module.rootPath)}`);
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
		{#if standups.length === 0 && contents?.folders?.length === 0}
			<EmptyState
				icon={FileText}
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
