<script lang="ts">
	import { createQuery, createMutation } from '$lib/query-compat';
	import { listAllFiles } from '$lib/api/files';
	import { listEnabledModules } from '$lib/api/modules';
	import { currentUser } from '$lib/stores/auth';

	import { formatFileSize, formatDate } from '$lib/utils/format';
	import type { File } from '$lib/api/types';
	import {
		FileText,
		HardDrive,
		Users,
		Plus,
		ArrowRight,
		Share2,
		FileDigit,
		Activity,
		LayoutGrid
	} from 'lucide-svelte';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import WorkspaceModules from '$lib/components/dashboard/WorkspaceModules.svelte';

	// Specific query for all user files to get accurate totals
	const allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles()
	});

	// Query for shared files
	const sharedFilesQuery = createQuery({
		queryKey: ['shares-received'],
		queryFn: async () => {
			const response = await fetch('/api/v1/shares/received');
			if (!response.ok) throw new Error('Failed to fetch shared files');
			return response.json();
		}
	});

	// Enabled modules for Workspace Modules section
	const enabledModulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	$: enabledModules = $enabledModulesQuery.data ?? [];

	$: sharedFiles = $sharedFilesQuery.data || [];

	$: totalFilesCount = $allFilesQuery.data?.length || 0;
	$: totalSizeUsed =
		$allFilesQuery.data?.reduce((sum: number, file: File) => sum + file.size, 0) || 0;

	$: greeting = (() => {
		const hour = new Date().getHours();
		if (hour < 12) return 'Good morning';
		if (hour < 18) return 'Good afternoon';
		return 'Good evening';
	})();

	function handleCreateNew() {
		window.location.href = '/files';
	}
</script>

<svelte:head>
	<title>Dashboard - RustShare</title>
</svelte:head>

{#if $allFilesQuery.isLoading && $enabledModulesQuery.isLoading}
	<DashboardSkeleton />
{:else}
	<!-- Main dashboard container - aligned with topbar "+ New" button via consistent padding -->
	<div class="dashboard-container">
		<!-- Workspace Overview Panel -->
		<section class="workspace-panel">
			<div class="workspace-panel-inner">
				<!-- Left: Greeting and overview -->
				<div class="workspace-greeting">
					<div class="workspace-badge">
						<Activity size={12} />
						<span>Workspace Overview</span>
					</div>
					<h1 class="workspace-title">
						{greeting},
						<span class="text-brand-500">{$currentUser?.display_name?.split(' ')[0] || 'User'}</span
						>.
					</h1>
					<p class="workspace-description">
						Everything in its right place. Monitor your storage velocity, access shared resources,
						and pick up exactly where you left off.
					</p>
				</div>

				<!-- Right: Storage and quick action -->
				<div class="workspace-actions">
					<div class="storage-card">
						<div class="storage-card-header">
							<span class="storage-label">Storage Stance</span>
							<HardDrive size={12} class="text-base-content/30" />
						</div>
						<p class="storage-value">
							{#if $currentUser?.storage_quota}
								{formatFileSize(totalSizeUsed)} of {formatFileSize($currentUser.storage_quota)} used
							{:else}
								{formatFileSize(totalSizeUsed)} used (No quota)
							{/if}
						</p>
						{#if $currentUser?.storage_quota}
							<div class="storage-progress">
								<div
									class="storage-progress-bar"
									style="width: {Math.min(
										100,
										(totalSizeUsed / $currentUser.storage_quota) * 100
									)}%"
								></div>
							</div>
						{/if}
					</div>

					<button on:click={handleCreateNew} class="action-button">
						<div class="action-button-content">
							<div class="action-button-icon">
								<Plus size={18} />
							</div>
							<div class="action-button-text">
								<p class="action-button-label">Action</p>
								<p class="action-button-value">Create New Item</p>
							</div>
						</div>
						<ArrowRight size={16} class="action-button-arrow" />
					</button>
				</div>
			</div>

			<!-- Compact Stats Grid - Embedded inside Workspace Overview -->
			<div class="workspace-stats">
				<div class="stat-box">
					<div class="stat-box-header">
						<span class="stat-box-label">Total Files</span>
						<div class="stat-box-icon stat-box-icon-info">
							<FileText size={14} />
						</div>
					</div>
					<p class="stat-box-value">{totalFilesCount}</p>
				</div>

				<div class="stat-box">
					<div class="stat-box-header">
						<span class="stat-box-label">Shared Items</span>
						<div class="stat-box-icon stat-box-icon-brand">
							<Share2 size={14} />
						</div>
					</div>
					<p class="stat-box-value">{sharedFiles.length}</p>
				</div>

				<div class="stat-box">
					<div class="stat-box-header">
						<span class="stat-box-label">Quota Limit</span>
						<div class="stat-box-icon stat-box-icon-warning">
							<FileDigit size={14} />
						</div>
					</div>
					<p class="stat-box-value">
						{#if $currentUser?.storage_quota}
							{formatFileSize($currentUser.storage_quota)}
						{:else}
							None
						{/if}
					</p>
				</div>

				<div class="stat-box">
					<div class="stat-box-header">
						<span class="stat-box-label">Modules</span>
						<div class="stat-box-icon stat-box-icon-brand">
							<LayoutGrid size={14} />
						</div>
					</div>
					<p class="stat-box-value">{enabledModules.length}</p>
				</div>
			</div>
		</section>

		<!-- Workspace Modules -->
		<WorkspaceModules modules={enabledModules} />

		<!-- Shared With Me Section -->
		{#if sharedFiles.length > 0}
			<section class="shared-panel">
				<div class="shared-panel-header">
					<div class="shared-panel-title-row">
						<Users size={14} class="text-base-content/40" />
						<h2 class="shared-panel-title">Shared With Me</h2>
					</div>
				</div>
				<div class="shared-list">
					{#each sharedFiles.slice(0, 5) as share}
						<a href="/files?folder={share.resource_id}" class="shared-item">
							<div class="shared-item-icon">
								<Share2 size={14} />
							</div>
							<div class="shared-item-content">
								<p class="shared-item-name">{share.resource_name}</p>
								<p class="shared-item-meta">Shared by <span>{share.shared_by_name}</span></p>
							</div>
							<span class="shared-item-type">{share.resource_type}</span>
						</a>
					{/each}
				</div>
			</section>
		{/if}
	</div>
{/if}

<style>
	/* Dashboard Container - Aligned with topbar */
	.dashboard-container {
		max-width: 1200px;
		margin: 0 auto;
		padding: 0 1rem 2.5rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	@media (min-width: 640px) {
		.dashboard-container {
			padding: 0 1.5rem 2.5rem;
		}
	}

	@media (min-width: 1024px) {
		.dashboard-container {
			padding: 0 2rem 2.5rem;
		}
	}

	/* Workspace Overview Panel */
	.workspace-panel {
		background: linear-gradient(
			to bottom right,
			var(--base-100),
			var(--base-100),
			color-mix(in oklab, var(--base-200) 50%, transparent)
		);
		border: 1px solid color-mix(in oklab, var(--base-300) 60%, transparent);
		border-radius: 1.5rem;
		padding: 1.5rem;
		box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	@media (min-width: 640px) {
		.workspace-panel {
			padding: 2rem;
		}
	}

	.workspace-panel-inner {
		display: grid;
		gap: 2rem;
	}

	@media (min-width: 1024px) {
		.workspace-panel-inner {
			grid-template-columns: 1fr auto;
		}
	}

	.workspace-greeting {
		display: flex;
		flex-direction: column;
	}

	.workspace-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0.75rem;
		border-radius: 9999px;
		background: color-mix(in oklab, var(--brand-500) 10%, transparent);
		border: 1px solid color-mix(in oklab, var(--brand-500) 20%, transparent);
		color: var(--brand-600);
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		width: fit-content;
		margin-bottom: 1rem;
	}

	.workspace-title {
		font-family: 'Fraunces', Georgia, serif;
		font-size: 1.875rem;
		font-weight: 500;
		line-height: 1.2;
		letter-spacing: -0.025em;
		color: var(--base-content);
	}

	@media (min-width: 1024px) {
		.workspace-title {
			font-size: 2.25rem;
		}
	}

	.workspace-description {
		margin-top: 0.75rem;
		max-width: 36rem;
		font-size: 0.875rem;
		line-height: 1.625;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.workspace-actions {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		justify-content: center;
	}

	@media (min-width: 1024px) {
		.workspace-actions {
			min-width: 240px;
		}
	}

	.storage-card {
		background: color-mix(in oklab, var(--base-100) 50%, transparent);
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		border-radius: 1rem;
		padding: 1rem;
		transition: border-color 0.2s;
	}

	.storage-card:hover {
		border-color: var(--base-300);
	}

	.storage-card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.25rem;
	}

	.storage-label {
		font-size: 0.625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
	}

	.storage-value {
		font-family: 'IBM Plex Mono', monospace;
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--base-content);
	}

	.storage-progress {
		margin-top: 0.5rem;
		height: 0.25rem;
		width: 100%;
		background: var(--base-300);
		border-radius: 9999px;
		overflow: hidden;
	}

	.storage-progress-bar {
		height: 100%;
		background: var(--brand-500);
		border-radius: 9999px;
		transition: width 1s ease-out;
	}

	.action-button {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 1rem;
		background: color-mix(in oklab, var(--brand-500) 5%, transparent);
		border: 1px solid color-mix(in oklab, var(--brand-500) 20%, transparent);
		border-radius: 1rem;
		color: var(--brand-600);
		transition: all 0.2s;
		text-align: left;
	}

	.action-button:hover {
		background: color-mix(in oklab, var(--brand-500) 10%, transparent);
	}

	.action-button-content {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.action-button-icon {
		display: flex;
		height: 2rem;
		width: 2rem;
		align-items: center;
		justify-content: center;
		border-radius: 0.75rem;
		background: var(--brand-500);
		color: white;
		box-shadow: 0 10px 15px -3px color-mix(in oklab, var(--brand-500) 30%, transparent);
	}

	.action-button-label {
		font-size: 0.625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		opacity: 0.6;
	}

	.action-button-value {
		font-family: 'IBM Plex Mono', monospace;
		font-size: 0.75rem;
		font-weight: 700;
	}

	/* Compact Stats Grid */
	.workspace-stats {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
		padding-top: 1.25rem;
		border-top: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
	}

	@media (min-width: 640px) {
		.workspace-stats {
			grid-template-columns: repeat(4, 1fr);
		}
	}

	@media (min-width: 640px) {
		.workspace-stats {
			gap: 1rem;
		}
	}

	.stat-box {
		background: var(--base-100);
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		border-radius: 0.75rem;
		padding: 0.875rem;
		transition: all 0.2s;
	}

	.stat-box:hover {
		border-color: var(--base-300);
		box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
	}

	.stat-box-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}

	.stat-box-label {
		font-size: 0.625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
	}

	.stat-box-icon {
		display: flex;
		height: 1.5rem;
		width: 1.5rem;
		align-items: center;
		justify-content: center;
		border-radius: 0.5rem;
		transition: all 0.2s;
	}

	.stat-box:hover .stat-box-icon-info {
		background: var(--info);
		color: white;
	}

	.stat-box:hover .stat-box-icon-brand {
		background: var(--brand-500);
		color: white;
	}

	.stat-box:hover .stat-box-icon-warning {
		background: var(--warning);
		color: white;
	}

	.stat-box-icon-info {
		background: color-mix(in oklab, var(--info) 10%, transparent);
		color: var(--info);
	}

	.stat-box-icon-brand {
		background: color-mix(in oklab, var(--brand-500) 10%, transparent);
		color: var(--brand-500);
	}

	.stat-box-icon-warning {
		background: color-mix(in oklab, var(--warning) 10%, transparent);
		color: var(--warning);
	}

	.stat-box-value {
		font-family: 'Instrument Sans', system-ui, sans-serif;
		font-size: 1.25rem;
		font-weight: 600;
		line-height: 1.2;
		letter-spacing: -0.025em;
		color: var(--base-content);
	}

	@media (min-width: 640px) {
		.stat-box-value {
			font-size: 1.5rem;
		}
	}

	/* Shared Panel */
	.shared-panel {
		background: var(--base-100);
		border: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
		border-radius: 1.5rem;
		overflow: hidden;
		box-shadow: 0 1px 3px 0 rgb(0 0 0 / 0.05);
	}

	.shared-panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 1rem;
		background: color-mix(in oklab, var(--base-200) 20%, transparent);
		border-bottom: 1px solid color-mix(in oklab, var(--base-300) 50%, transparent);
	}

	.shared-panel-title-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.shared-panel-title {
		font-size: 0.75rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	.shared-list {
		display: flex;
		flex-direction: column;
	}

	.shared-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		transition: background-color 0.15s;
	}

	.shared-item:hover {
		background: color-mix(in oklab, var(--base-200) 40%, transparent);
	}

	.shared-item:not(:last-child) {
		border-bottom: 1px solid color-mix(in oklab, var(--base-300) 30%, transparent);
	}

	.shared-item-icon {
		display: flex;
		height: 2rem;
		width: 2rem;
		align-items: center;
		justify-content: center;
		border-radius: 0.75rem;
		background: color-mix(in oklab, var(--info) 10%, transparent);
		color: var(--info);
		flex-shrink: 0;
	}

	.shared-item-content {
		flex: 1;
		min-width: 0;
	}

	.shared-item-name {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.shared-item-meta {
		font-size: 0.75rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.shared-item-meta span {
		font-weight: 600;
	}

	.shared-item-type {
		font-size: 0.625rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: -0.025em;
		color: color-mix(in oklab, var(--base-content) 30%, transparent);
	}

	:global(.font-display) {
		font-family: 'Fraunces', Georgia, serif;
	}

	:global(.font-data) {
		font-family: 'IBM Plex Sans', system-ui, sans-serif;
	}

	:global(.text-brand-500) {
		color: var(--brand-500);
	}
</style>
