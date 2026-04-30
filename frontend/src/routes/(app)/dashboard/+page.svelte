<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { listAllFiles } from '$lib/api/files';
	import { listEnabledModules } from '$lib/api/modules';
	import { currentUser } from '$lib/stores/auth';
	import { formatFileSize } from '$lib/utils/format';
	import type { File, ModuleConfig } from '$lib/api/types';
	import { FileText, HardDrive, Plus, Share2, LayoutGrid, FileDigit } from 'lucide-svelte';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import WorkspaceModules from '$lib/components/dashboard/WorkspaceModules.svelte';
	import { goto } from '$app/navigation';
	import { runModulePrimaryAction } from '$lib/modules/moduleActions';

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

	const enabledModulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	$: enabledModules = $enabledModulesQuery.data ?? [];
	$: orderedDashboardModules = enabledModules
		.filter((module) => module.ui_config?.dashboard?.enabled !== false)
		.sort((a, b) => (a.ui_config?.dashboard?.order ?? 99) - (b.ui_config?.dashboard?.order ?? 99));
	$: primaryDashboardModule = orderedDashboardModules.find(
		(module) => module.ui_config?.dashboard?.primaryAction
	);
	$: sharedFiles = $sharedFilesQuery.data || [];
	$: totalFilesCount = $allFilesQuery.data?.length || 0;
	$: totalSizeUsed =
		$allFilesQuery.data?.reduce((sum: number, file: File) => sum + file.size, 0) || 0;
	$: storageQuota = $currentUser?.storage_quota ?? null;
	$: storagePercent = storageQuota ? Math.min(100, (totalSizeUsed / storageQuota) * 100) : 0;

	const summaryCards = [
		{
			label: 'Total Files',
			value: () => String(totalFilesCount),
			icon: FileText
		},
		{
			label: 'Shared Items',
			value: () => String(sharedFiles.length),
			icon: Share2
		},
		{
			label: 'Storage Usage',
			value: () => formatFileSize(totalSizeUsed),
			icon: HardDrive
		},
		{
			label: 'Quota',
			value: () => (storageQuota ? formatFileSize(storageQuota) : 'None'),
			icon: FileDigit
		},
		{
			label: 'Enabled Modules',
			value: () => String(enabledModules.length),
			icon: LayoutGrid
		}
	];

	async function handleCreateNew() {
		await goto('/files');
	}

	async function handlePrimaryModuleAction(module: ModuleConfig) {
		await runModulePrimaryAction(module, module.ui_config?.dashboard?.primaryAction);
	}
</script>

<svelte:head>
	<title>Dashboard - RustShare</title>
</svelte:head>

{#if $allFilesQuery.isLoading && $enabledModulesQuery.isLoading}
	<DashboardSkeleton />
{:else}
	<div class="dashboard-shell">
		<section class="summary-panel">
			<div class="summary-header">
				<div class="summary-copy">
					<p class="summary-kicker">Workspace Summary</p>
					<h1 class="summary-title">File-backed work at a glance</h1>
					<p class="summary-text">
						Track storage, sharing, and enabled modules from the registry-driven workspace surface.
					</p>
				</div>

				<div class="summary-actions">
					{#if primaryDashboardModule}
						<button
							type="button"
							class="summary-action"
							on:click={() => handlePrimaryModuleAction(primaryDashboardModule)}
						>
							<LayoutGrid size={16} />
							<span
								>{primaryDashboardModule.ui_config?.dashboard?.primaryAction?.label ?? 'Open'}</span
							>
						</button>
					{/if}

					<button
						type="button"
						class="summary-action summary-action-secondary"
						on:click={handleCreateNew}
					>
						<Plus size={16} />
						<span>New</span>
					</button>
				</div>
			</div>

			<div class="summary-grid">
				{#each summaryCards as card}
					<div class="summary-card">
						<div class="summary-card-top">
							<span class="summary-card-label">{card.label}</span>
							<span class="summary-card-icon">
								<svelte:component this={card.icon} size={14} />
							</span>
						</div>
						<p class="summary-card-value">{card.value()}</p>

						{#if card.label === 'Storage Usage' && storageQuota}
							<div class="summary-progress">
								<div class="summary-progress-bar" style={`width: ${storagePercent}%`}></div>
							</div>
						{/if}
					</div>
				{/each}
			</div>
		</section>

		<WorkspaceModules modules={enabledModules} />

		<section class="shared-panel">
			<div class="shared-header">
				<h2 class="shared-title">Shared With Me</h2>
				<p class="shared-subtitle">Recently shared workspace items.</p>
			</div>

			{#if sharedFiles.length === 0}
				<EmptyState
					title="No shared items"
					description="Items shared with you will appear here."
					icon={Share2}
				/>
			{:else}
				<div class="shared-list">
					{#each sharedFiles.slice(0, 5) as share}
						<a href="/files?folder={share.resource_id}" class="shared-item">
							<div class="shared-item-icon">
								<Share2 size={14} />
							</div>
							<div class="shared-item-copy">
								<p class="shared-item-name">{share.resource_name}</p>
								<p class="shared-item-meta">Shared by {share.shared_by_name}</p>
							</div>
							<span class="shared-item-type">{share.resource_type}</span>
						</a>
					{/each}
				</div>
			{/if}
		</section>
	</div>
{/if}

<style>
	.dashboard-shell {
		max-width: 1200px;
		margin: 0 auto;
		padding: 0 1rem 2.5rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	@media (min-width: 640px) {
		.dashboard-shell {
			padding: 0 1.5rem 2.5rem;
		}
	}

	@media (min-width: 1024px) {
		.dashboard-shell {
			padding: 0 2rem 2.5rem;
		}
	}

	.summary-panel,
	.shared-panel {
		background: var(--base-100);
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		border-radius: 1.5rem;
		padding: 1.25rem;
		box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
	}

	.summary-header {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	@media (min-width: 900px) {
		.summary-header {
			flex-direction: row;
			align-items: flex-start;
			justify-content: space-between;
		}
	}

	.summary-kicker {
		font-size: 0.75rem;
		font-weight: 700;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--brand-500);
		margin-bottom: 0.4rem;
	}

	.summary-title {
		font-size: clamp(1.4rem, 2vw, 1.9rem);
		font-weight: 700;
		color: var(--base-content);
	}

	.summary-text {
		margin-top: 0.35rem;
		max-width: 42rem;
		font-size: 0.92rem;
		color: color-mix(in oklab, var(--base-content) 65%, transparent);
	}

	.summary-action {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		align-self: flex-start;
		border-radius: 999px;
		border: 1px solid color-mix(in oklab, var(--brand-500) 22%, transparent);
		background: color-mix(in oklab, var(--brand-500) 10%, var(--base-100));
		color: var(--brand-600);
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.7rem 1rem;
		transition:
			background 0.2s ease,
			transform 0.2s ease;
	}

	.summary-action:hover {
		background: color-mix(in oklab, var(--brand-500) 16%, var(--base-100));
		transform: translateY(-1px);
	}

	.summary-action-secondary {
		background: color-mix(in oklab, var(--base-content) 4%, var(--base-100));
		border-color: color-mix(in oklab, var(--base-content) 12%, transparent);
		color: var(--base-content);
	}

	.summary-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-self: flex-start;
	}

	.summary-grid {
		display: grid;
		grid-template-columns: repeat(1, minmax(0, 1fr));
		gap: 0.75rem;
	}

	@media (min-width: 640px) {
		.summary-grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}

	@media (min-width: 1024px) {
		.summary-grid {
			grid-template-columns: repeat(5, minmax(0, 1fr));
		}
	}

	.summary-card {
		border-radius: 1.1rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 55%, transparent);
		background: color-mix(in oklab, var(--base-200) 35%, var(--base-100));
		padding: 0.95rem;
	}

	.summary-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		margin-bottom: 0.6rem;
	}

	.summary-card-label {
		font-size: 0.72rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: color-mix(in oklab, var(--base-content) 48%, transparent);
	}

	.summary-card-icon {
		color: color-mix(in oklab, var(--base-content) 42%, transparent);
	}

	.summary-card-value {
		font-size: 1.15rem;
		font-weight: 650;
		color: var(--base-content);
	}

	.summary-progress {
		margin-top: 0.75rem;
		height: 0.45rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--base-300) 70%, transparent);
		overflow: hidden;
	}

	.summary-progress-bar {
		height: 100%;
		border-radius: inherit;
		background: linear-gradient(90deg, var(--brand-400), var(--brand-500));
	}

	.shared-header {
		margin-bottom: 0.9rem;
	}

	.shared-title {
		font-size: 0.95rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.shared-subtitle {
		margin-top: 0.2rem;
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.shared-list {
		display: flex;
		flex-direction: column;
		gap: 0.65rem;
	}

	.shared-item {
		display: flex;
		align-items: center;
		gap: 0.85rem;
		border-radius: 1rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 45%, transparent);
		padding: 0.85rem 0.95rem;
		transition:
			border-color 0.2s ease,
			background 0.2s ease;
	}

	.shared-item:hover {
		border-color: color-mix(in oklab, var(--brand-500) 35%, transparent);
		background: color-mix(in oklab, var(--base-200) 35%, transparent);
	}

	.shared-item-icon {
		display: flex;
		height: 2rem;
		width: 2rem;
		align-items: center;
		justify-content: center;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 12%, transparent);
		color: var(--brand-500);
	}

	.shared-item-copy {
		min-width: 0;
		flex: 1;
	}

	.shared-item-name {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--base-content);
	}

	.shared-item-meta {
		font-size: 0.78rem;
		color: color-mix(in oklab, var(--base-content) 48%, transparent);
	}

	.shared-item-type {
		font-size: 0.72rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: color-mix(in oklab, var(--base-content) 48%, transparent);
	}
</style>
