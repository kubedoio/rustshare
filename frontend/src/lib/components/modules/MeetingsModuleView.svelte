<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { Folder, Plus, Clock } from 'lucide-svelte';

	export let moduleConfig: {
		module_key: string;
		display_name: string;
		description: string;
		icon: string;
		root_path: string;
		default_template: string | null;
		ui_config?: {
			modulePage?: {
				emptyStateTitle?: string;
				emptyStateDescription?: string;
				emptyStateAction?: string;
			};
		};
	};

	$: emptyTitle = moduleConfig.ui_config?.modulePage?.emptyStateTitle ?? 'No meetings yet';
	$: emptyDescription =
		moduleConfig.ui_config?.modulePage?.emptyStateDescription ??
		'Create your first meeting note to get started.';
	$: emptyAction = moduleConfig.ui_config?.modulePage?.emptyStateAction ?? 'New Meeting';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['meetings-root', moduleConfig.module_key],
		queryFn: async () => {
			const res = await fetch('/api/v1/folders/root/contents');
			if (!res.ok) throw new Error('Failed to fetch root contents');
			const data = await res.json();
			const rootName = moduleConfig.root_path.replace(/^\//, '');
			const folder = data.folders?.find((f: { name: string }) => f.name === rootName);
			if (!folder) return { folders: [], files: [], current_folder: null };
			const contents = await getFolderContents(folder.id);
			return { ...contents, current_folder: folder };
		},
		enabled: true
	});

	$: contents = $rootFolderQuery.data;
	$: meetings = contents?.folders ?? [];

	async function handleCreateMeeting() {
		if (!moduleConfig.default_template) return;
		const name = window.prompt('Enter a name for the new meeting:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			if (result.object_type === 'folder') {
				goto(`/files?folder=${result.object_id}`);
			}
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create meeting:', err);
		}
	}

	function navigateToMeeting(folderId: string) {
		goto(`/files?folder=${folderId}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if meetings.length === 0 && contents?.files?.length === 0}
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
									{new Date(meeting.updated_at).toLocaleDateString()}
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
