<script lang="ts">
	import { goto } from '$app/navigation';
	import CompactWorkspaceOverview from '$lib/components/dashboard/CompactWorkspaceOverview.svelte';
	import WorkspaceSummaryInsightsSection from '$lib/components/dashboard/WorkspaceSummaryInsightsSection.svelte';
	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import { listAllFiles } from '$lib/api/files';
	import { currentUser } from '$lib/stores/auth';
	import { createQuery } from '$lib/query-compat';
	import type { File } from '$lib/api/types';
	import { Plus } from 'lucide-svelte';
	import { DEFAULT_WORKSPACE_SURFACE, getDashboardModulesForUser } from '$lib/modules/registry';

	const allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles()
	});

	const sharedFilesQuery = createQuery({
		queryKey: ['shares-received'],
		queryFn: async () => {
			const response = await fetch('/api/v1/shares/received');
			if (!response.ok) throw new Error('Failed to fetch shared files');
			return response.json();
		}
	});

	$: surface = DEFAULT_WORKSPACE_SURFACE;
	$: sections = surface.sections
		.filter((section) => section.enabled)
		.sort((a, b) => a.order - b.order);
	$: dashboardModules = getDashboardModulesForUser($currentUser);
	$: primaryDashboardModule = dashboardModules.find((module) => !!module.ui.dashboard.widget.primaryAction);
	$: sharedFiles = $sharedFilesQuery.data ?? [];
	$: totalFilesCount = $allFilesQuery.data?.length ?? 0;
	$: totalSizeUsed =
		$allFilesQuery.data?.reduce((sum: number, file: File) => sum + (file.size || 0), 0) ?? 0;
	$: storageQuota = $currentUser?.storage_quota ?? null;
	$: workspaceTitle = `${$currentUser?.display_name ?? 'Workspace'}'s Workspace Overview`;

	async function handleCreateNew() {
		await goto('/files');
	}

	async function handlePrimaryModuleAction(module: any) {
		const action = module.ui.dashboard.widget.primaryAction;
		if (action && action.action === 'create-from-template') {
			console.log('Creating from template:', action.template);
		}
	}
</script>

<svelte:head>
	<title>Workspace Dashboard - RustShare</title>
</svelte:head>

{#if $allFilesQuery.isLoading}
	<DashboardSkeleton />
{:else}
	<div class="workspace-dashboard-page">
		{#each sections as section (section.key)}
			{#if section.renderer === 'compact-workspace-overview'}
				<div class="overview-stack">
					<CompactWorkspaceOverview
						{workspaceTitle}
						totalFiles={totalFilesCount}
						sharedItems={sharedFiles.length}
						{storageQuota}
						storageUsed={totalSizeUsed}
					/>

					<div class="surface-actions">
						{#if primaryDashboardModule}
							<button
								type="button"
								class="surface-action primary"
								on:click={() => handlePrimaryModuleAction(primaryDashboardModule)}
							>
								{primaryDashboardModule.ui.dashboard.widget.primaryAction?.label ?? 'Open'}
							</button>
						{/if}

						<button type="button" class="surface-action secondary" on:click={handleCreateNew}>
							<Plus size={16} />
							<span>New</span>
						</button>
					</div>
				</div>
			{:else if section.renderer === 'workspace-widget-grid'}
				<WorkspaceSummaryInsightsSection {section} modules={dashboardModules} />
			{/if}
		{/each}
	</div>
{/if}

<style>
	.workspace-dashboard-page {
		max-width: 1320px;
		margin: 0 auto;
		padding: 0 2rem 2.75rem;
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	.overview-stack {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.surface-actions {
		display: flex;
		justify-content: flex-end;
		gap: 0.75rem;
	}

	.surface-action {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		border-radius: 999px;
		padding: 0.85rem 1.25rem;
		font-size: 0.92rem;
		font-weight: 700;
		border: 1px solid transparent;
		cursor: pointer;
	}

	.surface-action.primary {
		background: var(--brand-500);
		color: white;
		box-shadow: 0 10px 24px rgb(195 106 40 / 0.2);
	}

	.surface-action.secondary {
		background: color-mix(in oklab, var(--base-100) 90%, white);
		border-color: color-mix(in oklab, var(--base-300) 55%, transparent);
		color: var(--base-content);
	}

	@media (max-width: 1199px) {
		.workspace-dashboard-page {
			padding: 0 1.5rem 2.5rem;
		}
	}

	@media (max-width: 767px) {
		.workspace-dashboard-page {
			padding: 0 1rem 2rem;
			gap: 1.5rem;
		}

		.surface-actions {
			justify-content: stretch;
			flex-direction: column;
		}
	}
</style>
