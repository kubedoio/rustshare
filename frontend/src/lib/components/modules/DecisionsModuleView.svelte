<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import {
		FileText,
		Plus,
		Clock,
		Folder,
		Search,
		List,
		Grid3X3,
		ArrowUpDown,
		MoreHorizontal
	} from 'lucide-svelte';

	import { decisionsApi } from '$lib/api/decisions';
	import { activityStore } from '$lib/stores/activity';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	const decisionsQuery = createQuery({
		queryKey: ['decisions'],
		queryFn: () => decisionsApi.list()
	});

	let decisions = $derived($decisionsQuery.data ?? []);
	let searchTerm = $state('');
	let statusFilter = $state<'all' | 'accepted' | 'proposed' | 'superseded'>('all');
	let sortDirection = $state<'desc' | 'asc'>('desc');
	let viewMode = $state<'list' | 'grid'>('list');
	let itemsPerPage = $state(20);

	$effect(() => {
		viewMode = module.ui.page.layout === 'gallery-grid' ? 'grid' : 'list';
	});
	let filteredDecisions = $derived(
		decisions
			.filter((decision) =>
				(decision.metadata?.title || decision.name || '')
					.toLowerCase()
					.includes(searchTerm.trim().toLowerCase())
			)
			.filter((decision) => statusFilter === 'all' || decision.metadata?.status === statusFilter)
			.sort((a, b) => {
				const aTime = new Date(a.metadata?.decision_date ?? a.modified_at ?? 0).getTime();
				const bTime = new Date(b.metadata?.decision_date ?? b.modified_at ?? 0).getTime();
				return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
			})
	);
	let visibleDecisions = $derived(filteredDecisions.slice(0, itemsPerPage));

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
			activityStore.addActivity('decision_created', result.name || trimmed || 'Untitled Decision', {
				artifactId: result.id,
				moduleKey: 'decisions'
			});
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
	let searchPlaceholder = $derived(module.ui.page.searchPlaceholder ?? 'Search decisions...');
	let sortLabel = $derived(sortDirection === 'desc' ? 'Newest first' : 'Oldest first');
	let itemPlural = $derived(module.ui.page.itemPlural ?? 'decisions');

	function decisionTitle(decision: any): string {
		const code = decision.name?.match(/^DEC-\d+/)?.[0];
		const cleanTitle = (decision.metadata?.title || decision.name || '').replace(/\.md$/i, '');
		return code && !cleanTitle.startsWith(code) ? `${code} — ${cleanTitle}` : cleanTitle;
	}
</script>

<ModulePageShell title="Decisions" subtitle="Record important decisions with context and rationale.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleCreateDecision}
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
		{:else}
			<div class="overflow-hidden rounded-xl border border-base-300/60 bg-base-100">
				<div class="flex flex-col gap-3 border-b border-base-200 p-3 lg:flex-row lg:items-center">
					<label class="relative min-w-0 flex-1">
						<Search size={16} class="absolute top-1/2 left-3 -translate-y-1/2 text-base-content/35" />
						<input
							class="input-bordered input input-sm w-full pl-9"
							placeholder={searchPlaceholder}
							bind:value={searchTerm}
						/>
					</label>
					<select class="select-bordered select select-sm lg:w-40" bind:value={statusFilter} aria-label="Filter decisions">
						<option value="all">{module.ui.page.filterLabel ?? 'All decisions'}</option>
						<option value="accepted">Accepted</option>
						<option value="proposed">Proposed</option>
						<option value="superseded">Superseded</option>
					</select>
					<div class="ml-auto flex items-center gap-2">
						<button
							class="btn gap-2 btn-sm btn-outline"
							onclick={() => (sortDirection = sortDirection === 'desc' ? 'asc' : 'desc')}
						>
							<ArrowUpDown size={14} />
							<span>{sortLabel}</span>
						</button>
						<div class="join">
							<button
								class="btn join-item btn-sm {viewMode === 'list' ? 'btn-primary' : 'btn-outline'}"
								aria-label="List view"
								onclick={() => (viewMode = 'list')}
							>
								<List size={15} />
							</button>
							<button
								class="btn join-item btn-sm {viewMode === 'grid' ? 'btn-primary' : 'btn-outline'}"
								aria-label="Grid view"
								onclick={() => (viewMode = 'grid')}
							>
								<Grid3X3 size={15} />
							</button>
						</div>
					</div>
				</div>

				<div class={viewMode === 'grid' ? 'grid gap-3 p-3 sm:grid-cols-2 xl:grid-cols-3' : 'divide-y divide-base-200'}>
					{#each visibleDecisions as decision}
						<a
							href={`/modules/${module.key}/${decision.id}`}
							class={viewMode === 'grid'
								? 'rounded-xl border border-base-300/50 p-4 transition-colors hover:border-brand-500/30 hover:bg-base-200/30'
								: 'flex items-center gap-4 px-4 py-3 transition-colors hover:bg-base-200/40'}
						>
							<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-brand-500/10 text-brand-500 {viewMode === 'grid' ? 'mb-3' : ''}">
								<FileText size={16} />
							</div>
							<div class="flex min-w-0 flex-1 flex-col">
								<span class="truncate text-sm font-medium text-base-content">
									{decisionTitle(decision)}
								</span>
								<div class="flex items-center gap-2 text-xs text-base-content/55">
									{#if decision.metadata?.decision_date}
										<span>{new Date(decision.metadata.decision_date).toLocaleDateString()}</span>
									{/if}
									{#if decision.metadata?.status}
										<span class="capitalize">{decision.metadata.status}</span>
									{/if}
								</div>
							</div>
							<span class="{viewMode === 'grid' ? 'mt-3 block' : 'hidden lg:block'} max-w-xs truncate text-xs text-base-content/55">
								{decision.metadata?.category || 'General'}
							</span>
							{#if viewMode === 'list'}<MoreHorizontal size={16} class="text-base-content/45" />{/if}
						</a>
					{/each}
				</div>

				<div class="flex items-center justify-between border-t border-base-200 px-4 py-3 text-sm text-base-content/60">
					<span>{filteredDecisions.length} {itemPlural}</span>
					<label class="flex items-center gap-2">
						<span>Items per page</span>
						<select class="select-bordered select select-sm w-20" bind:value={itemsPerPage}>
							<option value={20}>20</option>
							<option value={50}>50</option>
						</select>
					</label>
				</div>
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
