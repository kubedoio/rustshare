<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { FileText, Plus, Clock, Folder, Users } from 'lucide-svelte';

	import { meetingsApi } from '$lib/api/meetings';
	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	let isGallery = $derived(module.ui.page.layout === 'gallery-grid');

	const meetingsQuery = createQuery({
		queryKey: ['meetings', module.key],
		queryFn: () => meetingsApi.list()
	});

	let meetings = $derived($meetingsQuery.data ?? []);

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

		const content = `# Meeting Notes\n\nDate:\nPeople:\n\n## Agenda\n\n## Notes\n\n## Decisions\n\n## Next steps\n`;

		try {
			const result = await meetingsApi.create({
				title,
				team: 'General',
				date: new Date().toISOString(),
				content
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
</script>

<ModulePageShell title="Meeting Notes" subtitle="Record simple meeting notes, decisions, and follow-up items.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleNewMeeting}
			disabled={isCreating || !module.defaultTemplate}
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
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each meetings as meeting}
					<a
						href={`/modules/${module.key}/${meeting.id}`}
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">
								{(meeting.metadata?.title || meeting.name || '').replace(/\.md$/i, '')}
							</span>
							{#if meeting.metadata?.date}
								<span class="flex items-center gap-1 text-xs text-base-content/40">
									<Clock size={12} />
									{new Date(meeting.metadata.date).toLocaleDateString()}
								</span>
							{/if}
							{#if meeting.metadata?.attendees?.length > 0}
								<span class="flex items-center gap-1 text-xs text-base-content/40">
									<Users size={12} />
									{meeting.metadata.attendees.length} people
								</span>
							{/if}
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{meeting.modified_at ? new Date(meeting.modified_at).toLocaleDateString() : ''}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-2">
				{#each meetings as meeting}
					<a
						href={`/modules/${module.key}/${meeting.id}`}
						class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
					>
						<div
							class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={16} />
						</div>
						<div class="flex min-w-0 flex-1 flex-col">
							<span class="text-sm font-medium text-base-content">
								{(meeting.metadata?.title || meeting.name || '').replace(/\.md$/i, '')}
							</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								{#if meeting.metadata?.date}
									<span class="inline-flex items-center gap-1">
										<Clock size={12} />
										{new Date(meeting.metadata.date).toLocaleDateString()}
									</span>
								{/if}
								{#if meeting.metadata?.attendees?.length > 0}
									<span class="inline-flex items-center gap-1">
										<Users size={12} />
										{meeting.metadata.attendees.length} people
									</span>
								{/if}
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{meeting.modified_at ? new Date(meeting.modified_at).toLocaleDateString() : ''}
								</span>
							</div>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>
