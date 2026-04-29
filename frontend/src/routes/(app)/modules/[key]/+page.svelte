<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listEnabledModules } from '$lib/api/modules';
	import { getFolderContents } from '$lib/api/folders';
	import { listRecentNotes, createNote } from '$lib/api/notes';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { Folder, FileText, ArrowLeft, Plus, AlertCircle, Clock } from 'lucide-svelte';

	$: moduleKey = $page.params.key;

	const enabledModulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	$: moduleConfig = $enabledModulesQuery.data?.find((m) => m.module_key === moduleKey);

	// Notes module: fetch recent notes for a dedicated landing view
	$: recentNotesQuery = createQuery({
		queryKey: ['recent-notes', moduleKey],
		queryFn: () => listRecentNotes(),
		enabled: moduleKey === 'notes' && !!moduleConfig?.enabled
	});

	// Fetch folder contents for the module root (non-notes modules)
	$: folderContentsQuery = createQuery({
		queryKey: ['module-folder-contents', moduleKey],
		queryFn: async () => {
			if (!moduleConfig) return null;
			const res = await fetch(`/api/v1/folders/root/contents`);
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			return { ...contents, current_folder: folder };
		},
		enabled: !!moduleConfig && moduleKey !== 'notes'
	});

	$: contents = $folderContentsQuery.data;
	$: recentNotes = $recentNotesQuery.data?.notes ?? [];
	$: isAvailable = moduleConfig?.enabled ?? false;
	$: isLoading = $enabledModulesQuery.isLoading;

	async function handleNewNote() {
		try {
			const data = await createNote({ title: 'Untitled Note' });
			goto(`/notes/${data.id}`);
		} catch (err) {
			console.error('Failed to create note:', err);
		}
	}
</script>

<svelte:head>
	<title>{moduleConfig?.display_name || 'Module'} - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-5xl p-4 lg:p-6">
	<a
		href="/dashboard"
		class="mb-4 inline-flex items-center gap-1.5 text-sm text-base-content/50 transition-colors hover:text-base-content"
	>
		<ArrowLeft size={14} />
		Back to Dashboard
	</a>

	{#if isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-spinner loading-lg text-brand-500"></div>
		</div>
	{:else if !isAvailable}
		<div class="flex flex-col items-center justify-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-12 text-center">
			<div class="flex h-16 w-16 items-center justify-center rounded-full bg-base-200 text-base-content/30">
				<AlertCircle size={32} />
			</div>
			<h1 class="text-xl font-semibold text-base-content">Module Not Available</h1>
			<p class="max-w-sm text-sm text-base-content/60">
				This module is currently disabled. Contact an administrator to enable it.
			</p>
			<a href="/dashboard" class="btn btn-primary btn-sm">Back to Dashboard</a>
		</div>
	{:else}
		<div class="flex flex-col gap-6">
			<!-- Module Header -->
			<div class="flex items-start gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-6">
				<div
					class="flex h-12 w-12 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
				>
					<ModuleIcon name={moduleConfig.icon} size={24} />
				</div>
				<div class="flex flex-col gap-1">
					<h1 class="text-lg font-semibold text-base-content">{moduleConfig.display_name}</h1>
					<p class="text-sm text-base-content/60">{moduleConfig.description}</p>
					<div class="mt-1 flex items-center gap-2">
						<span
							class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-base-content/50"
						>
							{moduleConfig.root_path}
						</span>
						<span
							class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-base-content/50"
						>
							{moduleConfig.renderer}
						</span>
					</div>
				</div>
			</div>

			<!-- Module Contents -->
			{#if moduleKey === 'notes'}
				<!-- Notes module: show recent notes landing -->
				<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6">
					<div class="mb-4 flex items-center justify-between">
						<h2 class="text-sm font-semibold uppercase tracking-wider text-base-content">
							Recent Notes
						</h2>
						<button class="btn btn-primary btn-sm" on:click={handleNewNote}>
							<Plus size={14} />
							<span>New Note</span>
						</button>
					</div>

					{#if $recentNotesQuery.isLoading}
						<div class="flex h-32 items-center justify-center">
							<div class="loading loading-spinner loading-md text-brand-500"></div>
						</div>
					{:else if recentNotes.length === 0}
						<EmptyState
							icon={FileText}
							title="No notes yet"
							description="Create your first note to get started."
							actionLabel="New Note"
							onAction={handleNewNote}
						/>
					{:else}
						<div class="flex flex-col gap-2">
							{#each recentNotes as note}
								<a
									href="/notes/{note.id}"
									class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
								>
									<div
										class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
									>
										<FileText size={16} />
									</div>
									<div class="flex flex-col">
										<span class="text-sm font-medium text-base-content">{note.metadata?.title || 'Untitled Note'}</span>
										<span class="flex items-center gap-1 text-xs text-base-content/40">
											<Clock size={12} />
											{note.modified_at ? new Date(note.modified_at).toLocaleDateString() : ''}
										</span>
									</div>
								</a>
							{/each}
						</div>
					{/if}
				</div>
			{:else}
				<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6">
					<div class="mb-4 flex items-center justify-between">
						<h2 class="text-sm font-semibold uppercase tracking-wider text-base-content">
							Contents
						</h2>
						<button class="btn btn-primary btn-sm">
							<Plus size={14} />
							<span>Create from Template</span>
						</button>
					</div>

					{#if $folderContentsQuery.isLoading}
						<div class="flex h-32 items-center justify-center">
							<div class="loading loading-spinner loading-md text-brand-500"></div>
						</div>
					{:else if !contents || (contents.folders?.length === 0 && contents.files?.length === 0)}
						<EmptyState
							icon={Folder}
							title="No items yet"
							description="Create your first item from a template to get started."
							actionLabel="Create from Template"
							onAction={() => {}}
						/>
					{:else}
						<div class="flex flex-col gap-2">
							{#if contents.folders?.length > 0}
								<div class="mb-2 text-xs font-semibold uppercase tracking-wider text-base-content/40">
									Folders
								</div>
								{#each contents.folders as folder}
									<a
										href="/files?folder={folder.id}"
										class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
									>
										<div
											class="flex h-9 w-9 items-center justify-center rounded-lg bg-info/10 text-info"
										>
											<Folder size={16} />
										</div>
										<div class="flex flex-col">
											<span class="text-sm font-medium text-base-content">{folder.name}</span>
											<span class="text-xs text-base-content/40">{folder.path}</span>
										</div>
									</a>
								{/each}
							{/if}

							{#if contents.files?.length > 0}
								<div class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wider text-base-content/40">
									Files
								</div>
								{#each contents.files as file}
									<a
										href="/files?preview={file.id}"
										class="flex items-center gap-3 rounded-xl border border-base-300/40 p-3 transition-colors hover:border-brand-500/30 hover:bg-base-200/30"
									>
										<div
											class="flex h-9 w-9 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
										>
											<FileText size={16} />
										</div>
										<div class="flex flex-col">
											<span class="text-sm font-medium text-base-content">{file.name}</span>
											<span class="text-xs text-base-content/40">{file.path}</span>
										</div>
									</a>
								{/each}
							{/if}
						</div>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</div>
