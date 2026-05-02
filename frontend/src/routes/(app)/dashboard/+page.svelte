<script lang="ts">
	import WorkspaceSummaryInsightsSection from '$lib/components/dashboard/WorkspaceSummaryInsightsSection.svelte';
	import DashboardSettingsPanel from '$lib/components/dashboard/DashboardSettingsPanel.svelte';
	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import { listAllFiles } from '$lib/api/files';
	import { currentUser } from '$lib/stores/auth';
	import { createQuery } from '$lib/query-compat';
	import { Settings } from 'lucide-svelte';
	import { DEFAULT_WORKSPACE_SURFACE, getDashboardModulesForUser } from '$lib/modules/registry';
	import { dashboardConfig, getVisibleModules } from '$lib/stores/dashboardConfig';

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
	$: dashboardConfig.hydrate(dashboardModules);
	$: visibleModules = getVisibleModules(dashboardModules, $dashboardConfig);
	$: sharedFiles = $sharedFilesQuery.data ?? [];
	$: totalFilesCount = $allFilesQuery.data?.length ?? 0;
</script>

<svelte:head>
	<title>Workspace Dashboard - RustShare</title>
</svelte:head>

{#if $allFilesQuery.isLoading}
	<DashboardSkeleton />
{:else}
	<div class="workspace-dashboard-page">
		<div class="dashboard-controls">
			<button
				type="button"
				class="settings-trigger"
				on:click={() => dashboardConfig.setEditMode(!$dashboardConfig.editMode)}
				title={$dashboardConfig.editMode ? 'Close settings' : 'Customize dashboard'}
				aria-pressed={$dashboardConfig.editMode}
			>
				<Settings size={18} />
			</button>
		</div>

		{#if $dashboardConfig.editMode}
			<DashboardSettingsPanel modules={dashboardModules} />
		{/if}

		{#each sections as section (section.key)}
			{#if section.renderer === 'workspace-widget-grid'}
				<WorkspaceSummaryInsightsSection modules={visibleModules} />
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
		gap: 1.5rem;
	}

	.dashboard-controls {
		display: flex;
		justify-content: flex-end;
	}

	.settings-trigger {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		border-radius: 0.85rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		background: color-mix(in oklab, var(--base-100) 92%, white);
		color: color-mix(in oklab, var(--base-content) 65%, transparent);
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background 150ms ease,
			color 150ms ease,
			border-color 150ms ease;
	}

	.settings-trigger:hover {
		border-color: var(--brand-500);
		color: var(--brand-500);
	}

	.settings-trigger[aria-pressed='true'] {
		background: var(--brand-500);
		border-color: var(--brand-500);
		color: white;
	}

	@media (max-width: 1199px) {
		.workspace-dashboard-page {
			padding: 0 1.5rem 2.5rem;
		}
	}

	@media (max-width: 767px) {
		.workspace-dashboard-page {
			padding: 0 1rem 2rem;
			gap: 1.25rem;
		}
	}
</style>
