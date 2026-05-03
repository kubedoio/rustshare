<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { FileText, Plus, Clock, Folder } from 'lucide-svelte';

	import { decisionsApi } from '$lib/api/decisions';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: isGallery = module.ui.page.layout === 'gallery-grid';

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No decisions yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Record your first decision to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Decision';

	// Fetch decisions via module service
	$: decisionsQuery = createQuery({
		queryKey: ['decisions', module.key],
		queryFn: () => decisionsApi.list()
	});

	$: decisions = $decisionsQuery.data ?? [];

	async function handleCreateDecision() {
		const title = window.prompt('Enter a title for the new decision:');
		if (!title) return;
		try {
			const result = await decisionsApi.create({
				title,
				category: 'General',
				content: '# ' + title + '\n\n'
			});
			goto(`/modules/${module.key}/${result.id}`);
			$decisionsQuery.refetch();
		} catch (err) {
			console.error('Failed to create decision:', err);
		}
	}

	function handleOpenInFiles() {
		if (module.rootPath) {
			goto(`/files?path=${encodeURIComponent(module.rootPath)}`);
		}
	}

	function navigateToDecision(id: string) {
		goto(`/modules/${module.key}/${id}`);
	}
</script>

<ModulePageShell title="Decisions" subtitle={module.description}>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateDecision}>
			<Plus size={14} />
			<span>New Decision</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if decisions.length === 0}
			<EmptyState
				icon={FileText}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateDecision}
			/>
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each decisions as decision}
					<button
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 bg-base-100 p-4 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
						onclick={() => navigateToDecision(decision.id)}
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{decision.name}</span>
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{new Date(decision.modified_at).toLocaleDateString()}
							</span>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-3">
				{#each decisions as decision}
					<button
						class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						onclick={() => navigateToDecision(decision.id)}
					>
						<div
							class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex min-w-0 flex-col gap-1">
							<span class="truncate text-sm font-medium text-base-content">{decision.name}</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{new Date(decision.modified_at).toLocaleDateString()}
								</span>
							</div>
						</div>
					</button>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>
