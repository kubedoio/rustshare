<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate, getModuleSummary } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { getModuleObjectHref } from '$lib/modules/modulePages';
	import { FileText, Plus, Clock } from 'lucide-svelte';

	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	const notesSummaryQuery = createQuery({
		queryKey: ['module-summary', 'notes-module-view', module.key],
		queryFn: () => getModuleSummary(module.key)
	});

	$: recentNotes = $notesSummaryQuery.data?.recent_items ?? [];

	async function handleNewNote() {
		if (!module.defaultTemplate) {
			return;
		}

		try {
			const result = await createFromTemplate({
				template_key: module.defaultTemplate,
				name: 'Untitled Note',
				parent_folder_id: null
			});
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
		} catch (err) {
			console.error('Failed to create note:', err);
		}
	}

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No notes yet';
	$: emptyDescription = module.ui.page.emptyStateDescription ?? 'Create your first note to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Note';
</script>

<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6">
	<div class="mb-4 flex items-center justify-between">
		<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Recent Notes</h2>
		<button
			class="btn btn-sm btn-primary"
			onclick={handleNewNote}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New Note</span>
		</button>
	</div>

	{#if $notesSummaryQuery.isLoading}
		<div class="flex h-32 items-center justify-center">
			<div class="loading loading-md loading-spinner text-brand-500"></div>
		</div>
	{:else if recentNotes.length === 0}
		<EmptyState
			icon={FileText}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleNewNote}
		/>
	{:else}
		<div class="flex flex-col gap-2">
			{#each recentNotes as note}
				<a
					href={getModuleObjectHref(module.key, 'file', note.id)}
					class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
				>
					<div
						class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
					>
						<FileText size={16} />
					</div>
					<div class="flex flex-col">
						<span class="text-sm font-medium text-base-content">{note.name}</span>
						<span class="flex items-center gap-1 text-xs text-base-content/40">
							<Clock size={12} />
							{note.updated_at ? new Date(note.updated_at).toLocaleDateString() : ''}
						</span>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
