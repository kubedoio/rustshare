<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { getModuleObjectHref, getModuleRootContents } from '$lib/modules/modulePages';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';
	import { FileText, Plus, Clock, Folder } from 'lucide-svelte';

	import { resolveModuleFolderId } from '$lib/modules/modulePages';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	let isGallery = $derived(module.ui.page.layout === 'gallery-grid');

	const contentsQuery = createQuery({
		queryKey: ['standups-root', module.key],
		queryFn: () => getModuleRootContents(module.rootPath)
	});

	let contents = $derived($contentsQuery.data);
	let standups = $derived(filterUserVisibleEntries(contents?.files ?? []));

	let createError = $state('');

	async function handleNewStandup() {
		createError = '';

		let title = `Standup — ${new Date().toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' })}`;
		const existingNames = standups.map((s) => s.name?.toLowerCase() ?? '');
		if (existingNames.includes(title.toLowerCase())) {
			let counter = 2;
			while (existingNames.includes(`${title} ${counter}`.toLowerCase())) {
				counter++;
			}
			title = `${title} ${counter}`;
		}

		if (!module.defaultTemplate) return;

		try {
			const result = await createFromTemplate({
				template_key: module.defaultTemplate,
				name: title,
				parent_folder_id: null
			});
			goto(getModuleObjectHref(module.key, result.object_type, result.object_id));
			$contentsQuery.refetch();
		} catch (err) {
			console.error('Failed to create standup:', err);
			createError = err instanceof Error ? err.message : 'Failed to create standup';
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

	let emptyTitle = $derived(module.ui.page.emptyStateTitle ?? 'No standup records yet');
	let emptyDescription = $derived(
		module.ui.page.emptyStateDescription ??
			'No standup records yet. Create a daily update to capture progress, blockers, and follow-up items.'
	);
	let emptyAction = $derived(module.ui.page.primaryAction?.label ?? 'New standup');
</script>

<ModulePageShell title="Standup Records" subtitle="Capture simple daily updates, blockers, and follow-up items.">
	<div slot="primaryAction">
		<button
			class="btn gap-2 btn-sm btn-primary"
			onclick={handleNewStandup}
			disabled={!module.defaultTemplate}
		>
			<Plus size={14} />
			<span>New standup</span>
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
		{#if $contentsQuery.isLoading}
			<div class="flex h-32 items-center justify-center">
				<div class="loading loading-md loading-spinner text-brand-500"></div>
			</div>
		{:else if standups.length === 0}
			<EmptyState
				icon={"📊"}
				title={emptyTitle}
				description={emptyDescription}
				actionLabel={emptyAction}
				onAction={handleNewStandup}
			/>
		{:else if isGallery}
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each standups as standup}
					<a
						href={getModuleObjectHref(module.key, 'file', standup.id)}
						class="group flex flex-col gap-3 rounded-xl border border-base-300/40 p-4 transition-all hover:border-brand-500/30 hover:bg-base-200/30 hover:shadow-sm"
					>
						<div
							class="flex h-10 w-10 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex flex-col">
							<span class="text-sm font-medium text-base-content">
								{standup.name.replace(/\.md$/i, '')}
							</span>
							<span class="flex items-center gap-1 text-xs text-base-content/40">
								<Clock size={12} />
								{new Date(standup.modified_at).toLocaleDateString()}
							</span>
						</div>
					</a>
				{/each}
			</div>
		{:else}
			<div class="flex flex-col gap-2">
				{#each standups as standup}
					<a
						href={getModuleObjectHref(module.key, 'file', standup.id)}
						class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
					>
						<div
							class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
						>
							<FileText size={16} />
						</div>
						<div class="flex min-w-0 flex-1 flex-col">
							<span class="text-sm font-medium text-base-content">
								{standup.name.replace(/\.md$/i, '')}
							</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{new Date(standup.modified_at).toLocaleDateString()}
								</span>
							</div>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	</div>
</ModulePageShell>
