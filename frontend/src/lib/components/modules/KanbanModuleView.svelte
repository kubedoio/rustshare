<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { Folder, Plus, ArrowRight, GripVertical } from 'lucide-svelte';

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

	$: emptyTitle = moduleConfig.ui_config?.modulePage?.emptyStateTitle ?? 'No boards yet';
	$: emptyDescription =
		moduleConfig.ui_config?.modulePage?.emptyStateDescription ??
		'Create your first kanban board to get started.';
	$: emptyAction = moduleConfig.ui_config?.modulePage?.emptyStateAction ?? 'New Board';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['kanban-root', moduleConfig.module_key],
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
	$: boards = contents?.folders ?? [];
	$: cards = contents?.files ?? [];

	async function handleCreateBoard() {
		if (!moduleConfig.default_template) return;
		const name = window.prompt('Enter a name for the new board:');
		if (!name) return;
		try {
			await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create board:', err);
		}
	}

	function navigateToBoard(folderId: string) {
		goto(`/files?folder=${folderId}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if boards.length === 0 && cards.length === 0}
		<EmptyState
			icon={Folder}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateBoard}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Boards</h2>
			<button class="btn btn-sm btn-primary" onclick={handleCreateBoard}>
				<Plus size={14} />
				<span>New Board</span>
			</button>
		</div>

		{#if boards.length > 0}
			<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
				{#each boards as board}
					<button
						class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						onclick={() => navigateToBoard(board.id)}
					>
						<div class="flex items-start justify-between">
							<div class="flex items-center gap-2">
								<GripVertical size={16} class="text-base-content/30" />
								<Folder size={18} class="text-brand-500" />
								<span class="text-sm font-medium text-base-content">{board.name}</span>
							</div>
							<ArrowRight
								size={14}
								class="text-base-content/30 transition-transform group-hover:translate-x-0.5"
							/>
						</div>
						<div class="flex items-center gap-2 text-xs text-base-content/50">
							<span>Updated {new Date(board.updated_at).toLocaleDateString()}</span>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">
				No boards yet. Create your first board to get started.
			</p>
		{/if}
	{/if}
</div>
