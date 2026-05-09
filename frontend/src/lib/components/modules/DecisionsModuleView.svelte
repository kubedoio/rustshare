<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import { FileText, Plus, Clock, Folder } from 'lucide-svelte';

	import { decisionsApi } from '$lib/api/decisions';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	let isGallery = $derived(module.ui.page.layout === 'gallery-grid');

	const decisionsQuery = createQuery({
		queryKey: ['decisions', module.key],
		queryFn: () => decisionsApi.list()
	});

	let decisions = $derived($decisionsQuery.data ?? []);

	let showPromptModal = $state(false);
	let createError = $state('');
	let isCreating = $state(false);

	async function handleCreateDecisionConfirm(title: string) {
		if (isCreating) return;
		const trimmed = title.trim();
		if (!trimmed) {
			createError = 'Title is required';
			return;
		}

		isCreating = true;
		createError = '';

		const content = `# Decision: ${trimmed}

## Context

## Decision

## Reason

## Follow-up

## Date
`;

		try {
			const result = await decisionsApi.create({
				title: trimmed,
				category: 'General',
				content
			});
			showPromptModal = false;
			createError = '';
			goto(`/modules/${module.key}/${result.id}`);
			$decisionsQuery.refetch();
		} catch (err) {
			console.error('Failed to create decision:', err);
			createError = err instanceof Error ? err.message : 'Failed to create decision';
		} finally {
			isCreating = false;
		}
	}

	function handleCreateDecision() {
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

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No decisions yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No decisions yet. Create a decision record to preserve context, rationale, and follow-up.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New decision');
</script>

<ModulePageShell title="Decisions" subtitle="Record important decisions with context and rationale.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleCreateDecision}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New decision</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if $decisionsQuery.isLoading}
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if decisions.length === 0}
			<EmptyState
				icon={"✅"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleCreateDecision}
			/>
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each decisions as decision}
					<a
						href={`/modules/${module.key}/${decision.id}`}
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">
								{decision.metadata?.title || decision.name}
							</span>
							<div class="flex flex-wrap items-center gap-2 text-xs text-base-content/50">
								{#if decision.name?.match(/^DEC-\d+/)?.[0]}
									<span class="inline-flex items-center gap-1 rounded bg-base-200 px-1.5 py-0.5 font-mono text-[10px]">
										{decision.name.match(/^DEC-\d+/)?.[0]}
									</span>
								{/if}
								{#if decision.metadata?.decision_date}
									<span class="inline-flex items-center gap-1">
										<Clock size={12} />
										{new Date(decision.metadata.decision_date).toLocaleDateString()}
									</span>
								{/if}
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{decision.modified_at ? new Date(decision.modified_at).toLocaleDateString() : ''}
								</span>
							</div>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-2">
				{#each decisions as decision}
					<a
						href={`/modules/${module.key}/${decision.id}`}
						class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
					>
						<div
							class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={16} />
						</div>
						<div class="flex min-w-0 flex-1 flex-col">
							<span class="text-sm font-medium text-base-content">
								{decision.metadata?.title || decision.name}
							</span>
							<div class="flex flex-wrap items-center gap-2 text-xs text-base-content/50">
								{#if decision.name?.match(/^DEC-\d+/)?.[0]}
									<span class="inline-flex items-center gap-1 rounded bg-base-200 px-1.5 py-0.5 font-mono text-[10px]">
										{decision.name.match(/^DEC-\d+/)?.[0]}
									</span>
								{/if}
								{#if decision.metadata?.decision_date}
									<span class="inline-flex items-center gap-1">
										<Clock size={12} />
										{new Date(decision.metadata.decision_date).toLocaleDateString()}
									</span>
								{/if}
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{decision.modified_at ? new Date(decision.modified_at).toLocaleDateString() : ''}
								</span>
							</div>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>

<PromptModal
	open={showPromptModal}
	title="New decision"
	message="Decision title"
	placeholder="e.g. Use file-backed workspace artifacts"
	confirmLabel="Create decision"
	error={createError}
	isLoading={isCreating}
	onConfirm={handleCreateDecisionConfirm}
	onCancel={() => {
		showPromptModal = false;
		createError = '';
	}}
/>
