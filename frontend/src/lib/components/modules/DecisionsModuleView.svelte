<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { FileText, Plus, Clock } from 'lucide-svelte';

	import { decisionsApi } from '$lib/api/decisions';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No decisions yet';
	$: emptyDescription = module.ui.page.emptyStateDescription ?? 'Record your first decision to get started.';
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

	function navigateToDecision(id: string) {
		goto(`/modules/${module.key}/${id}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if decisions.length === 0}
		<EmptyState
			icon={FileText}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateDecision}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Decisions</h2>
			<button class="btn btn-sm btn-primary" onclick={handleCreateDecision}>
				<Plus size={14} />
				<span>New Decision</span>
			</button>
		</div>

		{#if decisions.length > 0}
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
		{:else}
			<p class="text-sm text-base-content/50">
				No decisions yet. Record your first decision to get started.
			</p>
		{/if}
	{/if}
</div>
