<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { getFolderContents } from '$lib/api/folders';
	import { createFromTemplate } from '$lib/api/modules';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import { FileText, Plus, Clock } from 'lucide-svelte';

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

	$: emptyTitle = moduleConfig.ui_config?.modulePage?.emptyStateTitle ?? 'No decisions yet';
	$: emptyDescription =
		moduleConfig.ui_config?.modulePage?.emptyStateDescription ??
		'Record your first decision to get started.';
	$: emptyAction = moduleConfig.ui_config?.modulePage?.emptyStateAction ?? 'New Decision';

	// Fetch module root folder contents
	$: rootFolderQuery = createQuery({
		queryKey: ['decisions-root', moduleConfig.module_key],
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
	$: decisions = contents?.files ?? [];

	async function handleCreateDecision() {
		if (!moduleConfig.default_template) return;
		const name = window.prompt('Enter a name for the new decision:');
		if (!name) return;
		try {
			const result = await createFromTemplate({
				template_key: moduleConfig.default_template,
				name,
				parent_folder_id: null
			});
			if (result.object_type === 'file') {
				goto(`/files?preview=${result.object_id}`);
			}
			$rootFolderQuery.refetch();
		} catch (err) {
			console.error('Failed to create decision:', err);
		}
	}

	function navigateToDecision(fileId: string) {
		goto(`/files?preview=${fileId}`);
	}
</script>

<div class="flex flex-col gap-6">
	{#if decisions.length === 0 && contents?.folders?.length === 0}
		<EmptyState
			icon={FileText}
			title={emptyTitle}
			description={emptyDescription}
			actionLabel={emptyAction}
			onAction={handleCreateDecision}
		/>
	{:else}
		<div class="flex items-center justify-between">
			<h2 class="text-sm font-semibold tracking-wider text-base-content uppercase">Decisions</h2>
			<button class="btn btn-sm btn-primary" onclick={handleCreateDecision}>
				<Plus size={14} />
				<span>New Decision</span>
			</button>
		</div>

		{#if decisions.length > 0}
			<div class="flex flex-col gap-3">
				{#each decisions as decision}
					<button
						class="group flex items-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40 hover:shadow-md"
						onclick={() => navigateToDecision(decision.id)}
					>
						<div
							class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
						>
							<FileText size={18} />
						</div>
						<div class="flex min-w-0 flex-col gap-1">
							<span class="truncate text-sm font-medium text-base-content">{decision.name}</span>
							<div class="flex items-center gap-3 text-xs text-base-content/50">
								<span class="inline-flex items-center gap-1">
									<Clock size={12} />
									{new Date(decision.modified_at).toLocaleDateString()}
								</span>
							</div>
						</div>
					</button>
				{/each}
			</div>
		{:else}
			<p class="text-sm text-base-content/50">
				No decisions yet. Record your first decision to get started.
			</p>
		{/if}
	{/if}
</div>
