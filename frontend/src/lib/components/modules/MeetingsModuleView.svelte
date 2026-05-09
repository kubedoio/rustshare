<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { Folder, Plus, Clock } from 'lucide-svelte';

	import { meetingsApi } from '$lib/api/meetings';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	export let module: ModuleDefinition;

	$: isGallery = module.ui.page.layout === 'gallery-grid';

	$: emptyTitle = module.ui.page.emptyStateTitle ?? 'No meetings yet';
	$: emptyDescription =
		module.ui.page.emptyStateDescription ?? 'Create your first meeting to get started.';
	$: emptyAction = module.ui.page.primaryAction?.label ?? 'New Meeting';

	// Fetch meetings via module service
	$: meetingsQuery = createQuery({
		queryKey: ['meetings', module.key],
		queryFn: () => meetingsApi.list()
	});

	$: meetings = $meetingsQuery.data ?? [];

	let showPromptModal = false;

	async function handleCreateMeetingConfirm(title: string) {
		showPromptModal = false;
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

	function handleCreateMeeting() {
		showPromptModal = true;
	}

	async function handleOpenInFiles() {
		if (module.rootPath) {
			const folderId = await resolveModuleFolderId(module.rootPath);
			if (folderId) {
				goto(`/files?folder=${folderId}`);
			}
		}
	}

	function navigateToMeeting(id: string) {
		goto(`/modules/${module.key}/${id}`);
	}
</script>

<ModulePageShell title="Meeting Notes" subtitle={module.description}>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={handleCreateMeeting}>
			<Plus size={14} />
			<span>New Meeting</span>
		</button>
	</div>
	<div slot="secondaryActions">
		<button class="btn gap-2 btn-outline btn-sm" onclick={handleOpenInFiles}>
			<Folder size={14} />
			<span>Open in Files</span>
		</button>
	</div>

	<div class="flex flex-col gap-4">
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
				onAction={handleCreateMeeting}
			/>
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each meetings as meeting}
					<button
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 bg-base-100 p-4 text-left transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
						onclick={() => navigateToMeeting(meeting.id)}
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<Folder size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">{meeting.name}</span>
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{new Date(meeting.modified_at).toLocaleDateString()}
							</span>
						</div>
					</button>
				{/each}
			</div>
		{:else}
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
		{/if}
	</div>
</ModulePageShell>

<PromptModal
	open={showPromptModal}
	title="New Meeting"
	message="Enter a title for the new meeting:"
	confirmLabel="Create"
	onConfirm={handleCreateMeetingConfirm}
	onCancel={() => (showPromptModal = false)}
/>
