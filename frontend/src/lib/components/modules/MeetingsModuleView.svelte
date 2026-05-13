<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import {
		CalendarDays,
		Plus,
		Clock,
		Folder,
		Users,
		Search,
		List,
		Grid3X3,
		ArrowUpDown,
		MoreHorizontal
	} from 'lucide-svelte';

	import { meetingsApi } from '$lib/api/meetings';
	import { activityStore } from '$lib/stores/activity';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	const meetingsQuery = createQuery({
		queryKey: ['meetings'],
		queryFn: () => meetingsApi.list()
	});

	let meetings = $derived($meetingsQuery.data ?? []);
	let searchTerm = $state('');
	let statusFilter = $state<'all' | 'recent'>('all');
	let sortDirection = $state<'desc' | 'asc'>('desc');
	let viewMode = $state<'list' | 'grid'>('list');
	let itemsPerPage = $state(20);

	$effect(() => {
		viewMode = module.ui.page.layout === 'gallery-grid' ? 'grid' : 'list';
	});
	let filteredMeetings = $derived(
		meetings
			.filter((meeting) =>
				(meeting.metadata?.title || meeting.name || '')
					.toLowerCase()
					.includes(searchTerm.trim().toLowerCase())
			)
			.filter((meeting) => {
				if (statusFilter === 'all') return true;
				const timestamp = new Date(meeting.modified_at ?? meeting.metadata?.updated_at ?? 0).getTime();
				return timestamp >= Date.now() - 30 * 24 * 60 * 60 * 1000;
			})
			.sort((a, b) => {
				const aTime = new Date(a.modified_at ?? a.metadata?.updated_at ?? 0).getTime();
				const bTime = new Date(b.modified_at ?? b.metadata?.updated_at ?? 0).getTime();
				return sortDirection === 'desc' ? bTime - aTime : aTime - bTime;
			})
	);
	let visibleMeetings = $derived(filteredMeetings.slice(0, itemsPerPage));

	let createError = $state('');
	let isCreating = $state(false);

	async function handleNewMeeting() {
		if (isCreating) return;
		isCreating = true;
		createError = '';

		let title = 'Untitled Meeting Note';
		const existingNames = meetings.map((m) => m.name?.toLowerCase() ?? '');
		if (existingNames.includes(title.toLowerCase())) {
			let counter = 2;
			while (existingNames.includes(`${title} ${counter}`.toLowerCase())) {
				counter++;
			}
			title = `${title} ${counter}`;
		}

		const content = `# ${title}\n\n## Agenda\n- \n\n## Attendees\n- \n\n## Notes\n- \n\n## Decisions\n- \n\n## Action Items\n- [ ] \n`;

		try {
			const result = await meetingsApi.create({
				title,
				team: 'General',
				date: new Date().toISOString(),
				content
			});
			activityStore.addActivity('meeting_created', result.name || title || 'Untitled Meeting Note', {
				artifactId: result.id,
				moduleKey: 'meetings'
			});
			goto(`/modules/${module.key}/${result.id}`);
			$meetingsQuery.refetch();
		} catch (err) {
			console.error('Failed to create meeting:', err);
			createError = err instanceof Error ? err.message : 'Failed to create meeting';
		} finally {
			isCreating = false;
		}
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No meeting notes yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No meeting notes yet. Create a meeting note to capture agenda, discussion, decisions, and follow-up items.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New meeting note');
	let searchPlaceholder = $derived(module.ui.page.searchPlaceholder ?? 'Search meeting notes...');
	let sortLabel = $derived(sortDirection === 'desc' ? 'Modified' : 'Oldest first');
	let itemPlural = $derived(module.ui.page.itemPlural ?? 'meeting notes');
</script>

<ModulePageShell title="Meeting Notes" subtitle="Record simple meeting notes, decisions, and follow-up items.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleNewMeeting}
			disabled={isCreating}
		>
			<Plus size={14} />
			<span>New meeting note</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
		{#if createError}
			<div class="rounded-lg border border-red-300 bg-red-50 px-4 py-2 text-sm text-red-700">
				{createError}
			</div>
		{/if}
		{#if $meetingsQuery.isLoading}
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if meetings.length === 0}
			<EmptyState
				icon={"📅"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleNewMeeting}
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
					<select class="select-bordered select select-sm lg:w-40" bind:value={statusFilter} aria-label="Filter meetings">
						<option value="all">{module.ui.page.filterLabel ?? 'All notes'}</option>
						<option value="recent">Last 30 days</option>
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
					{#each visibleMeetings as meeting}
						<a
							href={`/modules/${module.key}/${meeting.id}`}
							class={viewMode === 'grid'
								? 'rounded-xl border border-base-300/50 p-4 transition-colors hover:border-brand-500/30 hover:bg-base-200/30'
								: 'flex items-center gap-4 px-4 py-3 transition-colors hover:bg-base-200/40'}
						>
							<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-brand-500/10 text-brand-500 {viewMode === 'grid' ? 'mb-3' : ''}">
								<CalendarDays size={16} />
							</div>
							<div class="flex min-w-0 flex-1 flex-col">
								<span class="truncate text-sm font-medium text-base-content">
									{(meeting.metadata?.title || meeting.name || '').replace(/\.md$/i, '')}
								</span>
								<div class="flex items-center gap-2 text-xs text-base-content/55">
									{#if meeting.metadata?.date}
										<span>{new Date(meeting.metadata.date).toLocaleDateString()}</span>
									{/if}
									{#if meeting.metadata?.attendees?.length > 0}
										<span>•</span>
										<span class="inline-flex items-center gap-1">
											<Users size={12} />
											{meeting.metadata.attendees.length} attendees
										</span>
									{/if}
								</div>
							</div>
							<span class="{viewMode === 'grid' ? 'mt-3 block' : 'hidden sm:block'} text-xs text-base-content/55">
								{meeting.modified_at ? new Date(meeting.modified_at).toLocaleDateString() : ''}
							</span>
							{#if viewMode === 'list'}<MoreHorizontal size={16} class="text-base-content/45" />{/if}
						</a>
					{/each}
				</div>

				<div class="flex items-center justify-between border-t border-base-200 px-4 py-3 text-sm text-base-content/60">
					<span>{filteredMeetings.length} {itemPlural}</span>
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
