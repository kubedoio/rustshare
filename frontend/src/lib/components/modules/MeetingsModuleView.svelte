<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { Folder, Plus, Clock } from 'lucide-svelte';

	import { meetingsApi } from '$lib/api/meetings';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No meetings yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Create your first meeting note to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Meeting';

	// Fetch meetings via module service
	$: meetingsQuery = createQuery({
		queryKey: ['meetings', module.key],
		queryFn: () => meetingsApi.list()
	});

	$: meetings = $meetingsQuery.data ?? [];

	async function handleCreateMeeting() {
		const title = window.prompt('Enter a title for the new meeting:');
		if (!title) return;
		try {
			const result = await meetingsApi.create({
				title,
				team: 'General',
				date: new Date().toISOString(),
				content: '# ' + title + '\n\n'
			});
			goto(`/modules/${module.key}/${result.id}`);
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to create meeting:', err);
		}
	}

	function navigateToMeeting(id: string) {
		goto(`/modules/${module.key}/${id}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if meetings.length === 0}
		<EmptyState
			icon={Folder}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateMeeting}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Meetings</h2>
			<button class="btn btn-sm btn-primary" onclick={handleCreateMeeting}>
				<Plus size={14} />
				<span>New Meeting</span>
			</button>
		</div>

		{#if meetings.length > 0}
			<div class="flex flex-col gap-3">
				{#each meetings as meeting}
					<button
						class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						onclick={() => navigateToMeeting(meeting.id)}
					>
						<div
							class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
						>
							<Folder size={18} />
						</div>
						<div class="flex min-w-0 flex-col gap-1">
							<span class="truncate text-sm font-medium text-base-content">{meeting.name}</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{new Date(meeting.modified_at).toLocaleDateString()}
								</span>
							</div>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">
				No meetings yet. Create your first meeting note to get started.
			</p>
		{/if}
	{/if}
</div>
