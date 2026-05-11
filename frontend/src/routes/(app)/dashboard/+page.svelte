<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listAllFiles } from '$lib/api/files';
	import { getStarredContents } from '$lib/api/files';
	import { createFromTemplate } from '$lib/api/modules';
	import { createNote } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { createBrainstormBoard } from '$lib/api/brainstorming';
	import { currentUser } from '$lib/stores/auth';
	import { activityStore, getActivityDisplay, getRelativeTime } from '$lib/stores/activity';
	import { filterUserVisibleEntries, isInternalRustShareFile } from '$lib/utils/artifactVisibility';
	import { formatDistanceToNow } from 'date-fns';
	import {
		StickyNote,
		CalendarDays,
		GitBranch,
		Columns,
		PenTool,
		Share2,
		FileText,
		Folder,
		Clock,
		Package,
		HardDrive,
		ArrowUpRight,
		Zap,
		ChevronRight,
		MoreVertical,
		Lightbulb,
		CheckCircle2,
		Type
	} from 'lucide-svelte';
	import { getModuleObjectHref } from '$lib/modules/modulePages';
	import { getEnabledModules } from '$lib/modules/registry';
	import type { ModuleSummary } from '$lib/api/types';
	import type { ModuleDefinition } from '$lib/modules/registry';
	import type { Folder as FolderType } from '$lib/api/types';
	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';

	// ---------------------------------------------------------------------------
	// Types
	// ---------------------------------------------------------------------------

	interface ArtifactItem {
		id: string;
		name: string;
		item_type: 'file' | 'folder';
		updated_at: string;
		moduleKey: string;
		moduleName: string;
	}

	interface QuickAction {
		label: string;
		subtitle: string;
		icon: any;
		iconColor: string;
		iconBg: string;
		onClick: () => void;
	}

	// ---------------------------------------------------------------------------
	// Helpers
	// ---------------------------------------------------------------------------

	function formatBytes(bytes: number): string {
		if (!bytes || bytes === 0) return '0 B';
		const k = 1024;
		const sizes = ['B', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
	}

	function getArtifactTypeLabel(moduleKey: string, itemType: string): string {
		const map: Record<string, string> = {
			notes: 'Note',
			meetings: 'Meeting Note',
			standups: 'Standup',
			kanban: 'Kanban Board',
			decisions: 'Decision',
			brainstorming: 'Idea Board',
			shares: 'Share'
		};
		return map[moduleKey] ?? (itemType === 'folder' ? 'Folder' : 'File');
	}

	function getArtifactHref(item: ArtifactItem): string {
		if (item.moduleKey === 'notes' && item.item_type === 'file') {
			return `/modules/notes/${item.id}`;
		}
		if (item.moduleKey === 'decisions') {
			return `/modules/decisions/${item.id}`;
		}
		if (item.item_type === 'folder') {
			return `/files?folder=${item.id}`;
		}
		return `/files?preview=${item.id}`;
	}

	function cleanArtifactName(name: string): string {
		return name.replace(/\.md$/i, '').replace(/\.jsonl?$/i, '');
	}

	function todayDateString(): string {
		return new Date().toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
	}

	function getUserInitials(name: string | undefined): string {
		if (!name) return '?';
		const parts = name.trim().split(/\s+/);
		if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
		return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
	}

	function getActivityVerb(type: string): string {
		switch (type) {
			case 'file_uploaded':
			case 'folder_created':
				return 'was created';
			case 'file_modified':
				return 'was updated';
			case 'file_downloaded':
				return 'was downloaded';
			case 'file_deleted':
			case 'folder_deleted':
				return 'was deleted';
			case 'file_renamed':
			case 'folder_renamed':
				return 'was renamed';
			case 'file_moved':
			case 'folder_moved':
				return 'was moved';
			case 'share_created':
				return 'was shared';
			case 'share_revoked':
				return 'share was revoked';
			default:
				return 'was updated';
		}
	}

	function getModuleColor(moduleKey: string): { color: string; bg: string } {
		const colors: Record<string, { color: string; bg: string }> = {
			notes: { color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' },
			meetings: { color: '#7c3aed', bg: 'rgba(124, 58, 237, 0.1)' },
			standups: { color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' },
			kanban: { color: '#ea580c', bg: 'rgba(234, 88, 12, 0.1)' },
			decisions: { color: '#16a34a', bg: 'rgba(22, 163, 74, 0.1)' },
			brainstorming: { color: '#ca8a04', bg: 'rgba(202, 138, 4, 0.1)' },
			shares: { color: '#2563eb', bg: 'rgba(37, 99, 235, 0.1)' }
		};
		return colors[moduleKey] ?? { color: '#6b7280', bg: 'rgba(107, 114, 128, 0.1)' };
	}

	function getArtifactIcon(moduleKey: string) {
		const map: Record<string, any> = {
			notes: FileText,
			meetings: FileText,
			standups: FileText,
			kanban: Columns,
			decisions: CheckCircle2,
			brainstorming: Lightbulb,
			shares: Share2
		};
		return map[moduleKey] ?? FileText;
	}

	// ---------------------------------------------------------------------------
	// Queries
	// ---------------------------------------------------------------------------

	const allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles()
	});

	const starredContentsQuery = createQuery({
		queryKey: ['starred-contents'],
		queryFn: () => getStarredContents()
	});

	const moduleSummariesQuery = createQuery({
		queryKey: ['workspace-module-summaries'],
		queryFn: async () => {
			const { getModuleSummary } = await import('$lib/api/modules');
			const modules = getEnabledModules();
			const results = await Promise.all(
				modules.map(async (m) => {
					try {
						const summary: ModuleSummary = await getModuleSummary(m.key);
						return { module: m, summary };
					} catch {
						return null;
					}
				})
			);
			return results.filter((r): r is { module: ModuleDefinition; summary: ModuleSummary } => r !== null);
		}
	});

	// ---------------------------------------------------------------------------
	// Derived state
	// ---------------------------------------------------------------------------

	let allFiles = $derived(filterUserVisibleEntries($allFilesQuery.data ?? []));

	let totalArtifacts = $derived(allFiles.length);

	let updatedThisWeek = $derived(() => {
		const weekAgo = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000);
		return allFiles.filter((f) => new Date(f.modified_at) >= weekAgo).length;
	});

	let sharedItemsCount = $derived(allFiles.filter((f) => f.is_shared).length);

	let storageUsed = $derived(formatBytes($currentUser?.storage_used ?? 0));
	let storageQuota = $derived(formatBytes($currentUser?.storage_quota ?? 0));

	let recentArtifacts = $derived(() => {
		const summaries = $moduleSummariesQuery.data ?? [];
		const items: ArtifactItem[] = [];

		for (const { module, summary } of summaries) {
			for (const item of summary.recent_items) {
				if (isInternalRustShareFile(item.name)) continue;
				items.push({
					id: item.id,
					name: item.name,
					item_type: item.item_type as 'file' | 'folder',
					updated_at: item.updated_at,
					moduleKey: module.key,
					moduleName: module.displayName
				});
			}
		}

		return items
			.sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime())
			.slice(0, 12);
	});

	let activities = $derived(($activityStore ?? []).slice(0, 10));

	let pinnedFolders = $derived(() => {
		const data = $starredContentsQuery.data;
		if (!data) return [];
		return (data.folders ?? []).slice(0, 5);
	});

	let isLoading = $derived($allFilesQuery.isLoading || $moduleSummariesQuery.isLoading);

	// ---------------------------------------------------------------------------
	// Quick actions
	// ---------------------------------------------------------------------------

	let creating = $state(false);
	let createError = $state('');

	async function handleNewNote() {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const result = await createNote({ title: 'Untitled Note', content: '# Untitled Note\n\n' });
			goto(`/modules/notes/${result.id}`);
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create note';
		} finally {
			creating = false;
		}
	}

	async function handleNewMeeting() {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const result = await createFromTemplate({
				template_key: 'template_default_meeting',
				name: 'Untitled Meeting Note',
				parent_folder_id: null
			});
			goto(getModuleObjectHref('meetings', result.object_type, result.object_id));
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create meeting note';
		} finally {
			creating = false;
		}
	}

	async function handleNewDecision() {
		if (creating) return;
		creating = true;
		createError = '';
		const title = `Decision — ${todayDateString()}`;
		const content = `# Decision: ${title}

## Context

## Decision

## Reason

## Follow-up

## Date
`;
		try {
			const result = await decisionsApi.create({ title, category: 'General', content });
			goto(`/modules/decisions/${result.id}`);
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create decision';
		} finally {
			creating = false;
		}
	}

	async function handleNewKanban() {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const result = await createFromTemplate({
				template_key: 'template_default_kanban',
				name: `Board — ${todayDateString()}`,
				parent_folder_id: null
			});
			goto(`/modules/kanban?boardId=${result.object_id}`);
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create board';
		} finally {
			creating = false;
		}
	}

	async function handleNewBrainstorm() {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const result = await createBrainstormBoard(`Idea Board — ${todayDateString()}`, 'template_blank_brainstorm');
			goto(`/modules/brainstorming/${result.id}`);
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create idea board';
		} finally {
			creating = false;
		}
	}

	function handleNewShare() {
		goto('/files');
	}

	const quickActions: QuickAction[] = [
		{
			label: 'New note',
			subtitle: 'Create a new note',
			icon: FileText,
			iconColor: '#ea580c',
			iconBg: 'rgba(234, 88, 12, 0.1)',
			onClick: handleNewNote
		},
		{
			label: 'New meeting note',
			subtitle: 'Record a meeting',
			icon: CalendarDays,
			iconColor: '#7c3aed',
			iconBg: 'rgba(124, 58, 237, 0.1)',
			onClick: handleNewMeeting
		},
		{
			label: 'New decision record',
			subtitle: 'Capture a decision',
			icon: CheckCircle2,
			iconColor: '#16a34a',
			iconBg: 'rgba(22, 163, 74, 0.1)',
			onClick: handleNewDecision
		},
		{
			label: 'New Kanban board',
			subtitle: 'Create a new board',
			icon: Columns,
			iconColor: '#ea580c',
			iconBg: 'rgba(234, 88, 12, 0.1)',
			onClick: handleNewKanban
		},
		{
			label: 'New idea board',
			subtitle: 'Brainstorm and capture ideas',
			icon: Lightbulb,
			iconColor: '#ca8a04',
			iconBg: 'rgba(202, 138, 4, 0.1)',
			onClick: handleNewBrainstorm
		},
		{
			label: 'New share',
			subtitle: 'Share files or folders',
			icon: Share2,
			iconColor: '#2563eb',
			iconBg: 'rgba(37, 99, 235, 0.1)',
			onClick: handleNewShare
		}
	];

	const summaryCards = $derived([
		{
			label: 'Total artifacts',
			value: totalArtifacts,
			subtitle: 'Across all sections',
			icon: Package,
			iconColor: '#ea580c',
			iconBg: 'rgba(234, 88, 12, 0.1)'
		},
		{
			label: 'Updated this week',
			value: updatedThisWeek(),
			subtitle: 'Files and records',
			icon: Clock,
			iconColor: '#16a34a',
			iconBg: 'rgba(22, 163, 74, 0.1)'
		},
		{
			label: 'Shared items',
			value: sharedItemsCount,
			subtitle: 'Active shares',
			icon: Share2,
			iconColor: '#7c3aed',
			iconBg: 'rgba(124, 58, 237, 0.1)'
		},
		{
			label: 'Storage used',
			value: storageUsed,
			subtitle: `of ${storageQuota}`,
			icon: HardDrive,
			iconColor: '#2563eb',
			iconBg: 'rgba(37, 99, 235, 0.1)'
		}
	]);
</script>

<svelte:head>
	<title>Workspace overview - RustShare</title>
</svelte:head>

{#if isLoading}
	<DashboardSkeleton />
{:else}
	<div class="workspace-overview-page">
		<!-- Header -->
		<header class="overview-header">
			<h1>Workspace overview</h1>
			<p class="overview-subtitle">Your company memory, organized and easy to find.</p>
		</header>

		{#if createError}
			<div class="rounded-lg border border-red-300 bg-red-50 px-4 py-2 text-sm text-red-700">
				{createError}
			</div>
		{/if}

		<div class="dashboard-grid">
			<!-- Left column -->
			<div class="dashboard-main">
				<!-- Summary cards -->
				<section class="summary-cards" aria-label="Workspace summary">
					{#each summaryCards as card}
						{@const SummaryIcon = card.icon}
						<div class="summary-card">
							<div class="summary-icon" style="background: {card.iconBg}; color: {card.iconColor};">
								<SummaryIcon size={18} />
							</div>
							<div class="summary-body">
								<span class="summary-value">{card.value}</span>
								<span class="summary-label">{card.label}</span>
								<span class="summary-subtitle">{card.subtitle}</span>
							</div>
						</div>
					{/each}
				</section>

				<!-- Recent artifacts -->
				<section class="recent-artifacts" aria-label="Recent artifacts">
					<div class="section-header">
						<h2 class="section-title">Recent artifacts</h2>
						<a href="/files" class="view-all-link">View all</a>
					</div>
					{#if recentArtifacts().length === 0}
						<div class="empty-state">
							<p class="empty-title">No recent artifacts yet.</p>
							<p class="empty-description">
								Create a note, meeting record, decision, or board to start building your workspace
								memory.
							</p>
						</div>
					{:else}
						<ul class="artifact-list">
							{#each recentArtifacts() as item}
								{@const modColor = getModuleColor(item.moduleKey)}
								{@const ArtifactIcon = getArtifactIcon(item.moduleKey)}
								<li>
									<a href={getArtifactHref(item)} class="artifact-link">
										<div class="artifact-icon" style="background: {modColor.bg}; color: {modColor.color};">
											<ArtifactIcon size={16} />
										</div>
										<div class="artifact-body">
											<span class="artifact-name">{cleanArtifactName(item.name)}</span>
											<div class="artifact-meta">
												<span class="artifact-type-badge">{getArtifactTypeLabel(item.moduleKey, item.item_type)}</span>
												<span class="artifact-time">
													{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}
												</span>
											</div>
										</div>
										<span class="artifact-user-avatar">
											{getUserInitials($currentUser?.display_name)}
										</span>
									</a>
								</li>
							{/each}
						</ul>
						<div class="view-all-row">
							<a href="/files" class="view-all-btn">
								View all recent artifacts <ArrowUpRight size={14} />
							</a>
						</div>
					{/if}
				</section>

				<!-- Recent activity -->
				<section class="recent-activity" aria-label="Recent activity">
					<div class="section-header">
						<h2 class="section-title">Recent activity</h2>
						<button class="view-all-link" onclick={() => goto('/settings?tab=activity')}>
							View all
						</button>
					</div>
					{#if activities.length === 0}
						<div class="empty-state minimal">
							<p class="empty-description">Activity will appear here as you work in your workspace.</p>
						</div>
					{:else}
						<ul class="activity-list">
							{#each activities as activity}
								{@const display = getActivityDisplay(activity)}
								<li class="activity-item">
									<div class="activity-icon-wrap">
										{display.icon}
									</div>
									<div class="activity-body">
										<span class="activity-text">
											<strong>{activity.fileName}</strong> {getActivityVerb(activity.type)}
										</span>
										<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
									</div>
									<span class="activity-user-avatar">
										{getUserInitials($currentUser?.display_name)}
									</span>
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			</div>

			<!-- Right column -->
			<div class="dashboard-sidebar">
				<!-- Quick actions -->
				<section class="quick-actions" aria-label="Quick actions">
					<h2 class="section-title">Quick actions</h2>
					<div class="action-list">
						{#each quickActions as action}
							{@const ActionIcon = action.icon}
							<button
								type="button"
								class="action-item"
								onclick={action.onClick}
								disabled={creating}
							>
								<div class="action-icon" style="background: {action.iconBg}; color: {action.iconColor};">
									<ActionIcon size={18} />
								</div>
								<div class="action-body">
									<span class="action-label">{action.label}</span>
									<span class="action-subtitle">{action.subtitle}</span>
								</div>
								<ChevronRight size={16} class="action-chevron" />
							</button>
						{/each}
					</div>
				</section>

				<!-- Pinned folders -->
				<section class="pinned-folders" aria-label="Pinned folders">
					<div class="section-header">
						<h2 class="section-title">Pinned folders</h2>
						<a href="/files" class="view-all-link">View all</a>
					</div>
					{#if pinnedFolders().length === 0}
						<div class="empty-state minimal">
							<p class="empty-description">
								Star folders to pin them here for quick access.
							</p>
						</div>
					{:else}
						<ul class="folder-list">
							{#each pinnedFolders() as folder}
								<li class="folder-item">
									<a href={`/files?folder=${folder.id}`} class="folder-link">
										<div class="folder-icon">
											<Folder size={18} />
										</div>
										<div class="folder-body">
											<span class="folder-name">{folder.name}</span>
											<span class="folder-path">{folder.path}</span>
										</div>
									</a>
									<button
										type="button"
										class="folder-menu-btn"
										aria-label="Folder options"
										onclick={(e) => e.preventDefault()}
									>
										<MoreVertical size={14} />
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			</div>
		</div>
	</div>
{/if}

<style>
	.workspace-overview-page {
		max-width: 1200px;
		margin: 0 auto;
		padding: 0 2rem 3rem;
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.overview-header {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding-top: 0.5rem;
	}

	.overview-header h1 {
		margin: 0;
		font-size: clamp(1.5rem, 3vw, 1.85rem);
		font-weight: 700;
		color: var(--base-content);
		font-family: 'Fraunces', serif;
		letter-spacing: -0.02em;
	}

	.overview-subtitle {
		margin: 0;
		font-size: 0.92rem;
		color: color-mix(in oklab, var(--base-content) 60%, transparent);
	}

	/* Dashboard grid */
	.dashboard-grid {
		display: grid;
		grid-template-columns: 1fr 340px;
		gap: 2rem;
		align-items: start;
	}

	.dashboard-main {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
	}

	.dashboard-sidebar {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
	}

	/* Section header */
	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.75rem;
	}

	.section-title {
		margin: 0;
		font-size: 0.9rem;
		font-weight: 700;
		color: var(--base-content);
	}

	.view-all-link {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--brand-500);
		text-decoration: none;
		background: none;
		border: none;
		cursor: pointer;
		padding: 0;
	}

	.view-all-link:hover {
		text-decoration: underline;
	}

	/* Summary cards */
	.summary-cards {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 0.875rem;
	}

	.summary-card {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem 1.1rem;
		border-radius: 0.875rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 45%, transparent);
		background: color-mix(in oklab, var(--base-100) 94%, white);
		box-shadow: 0 4px 16px rgb(72 42 17 / 0.04);
		transition: border-color 150ms ease;
	}

	.summary-card:hover {
		border-color: color-mix(in oklab, var(--brand-500) 25%, transparent);
	}

	.summary-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		border-radius: 0.65rem;
		flex-shrink: 0;
	}

	.summary-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
	}

	.summary-value {
		font-size: 1.35rem;
		font-weight: 700;
		color: var(--base-content);
		line-height: 1.2;
	}

	.summary-label {
		font-size: 0.78rem;
		font-weight: 600;
		color: var(--base-content);
	}

	.summary-subtitle {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	/* Recent artifacts */
	.artifact-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.artifact-link {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.65rem 0.85rem;
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		color: inherit;
		text-decoration: none;
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}

	.artifact-link:hover {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}

	.artifact-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.5rem;
		flex-shrink: 0;
	}

	.artifact-body {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
		flex: 1;
	}

	.artifact-name {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.artifact-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	.artifact-type-badge {
		padding: 0.1rem 0.45rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--base-300) 40%, transparent);
		font-size: 0.68rem;
		font-weight: 600;
		color: color-mix(in oklab, var(--base-content) 65%, transparent);
	}

	.artifact-time {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
	}

	.artifact-user-avatar {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 15%, transparent);
		color: var(--brand-500);
		font-size: 0.65rem;
		font-weight: 700;
		flex-shrink: 0;
	}

	.view-all-row {
		display: flex;
		justify-content: center;
		padding-top: 0.75rem;
	}

	.view-all-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--brand-500);
		text-decoration: none;
		transition: opacity 150ms ease;
	}

	.view-all-btn:hover {
		opacity: 0.8;
	}

	/* Quick actions sidebar */
	.action-list {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.action-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.65rem 0.75rem;
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		color: inherit;
		font-size: inherit;
		font-family: inherit;
		cursor: pointer;
		transition:
			border-color 150ms ease,
			background 150ms ease;
		text-align: left;
		width: 100%;
	}

	.action-item:hover:not(:disabled) {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}

	.action-item:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.action-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.5rem;
		flex-shrink: 0;
	}

	.action-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}

	.action-label {
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--base-content);
	}

	.action-subtitle {
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
	}

	:global(.action-chevron) {
		color: color-mix(in oklab, var(--base-content) 30%, transparent);
		flex-shrink: 0;
		transition: color 150ms ease;
	}

	.action-item:hover:not(:disabled) :global(.action-chevron) {
		color: var(--brand-500);
	}

	/* Activity */
	.activity-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.activity-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 0.75rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 50%, white);
	}

	.activity-icon-wrap {
		font-size: 1rem;
		line-height: 1;
		flex-shrink: 0;
		width: 1.75rem;
		text-align: center;
	}

	.activity-body {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
		flex: 1;
	}

	.activity-text {
		font-size: 0.8rem;
		color: var(--base-content);
		line-height: 1.4;
	}

	.activity-text strong {
		font-weight: 600;
	}

	.activity-time {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
	}

	.activity-user-avatar {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 999px;
		background: color-mix(in oklab, var(--brand-500) 15%, transparent);
		color: var(--brand-500);
		font-size: 0.65rem;
		font-weight: 700;
		flex-shrink: 0;
	}

	/* Pinned folders */
	.folder-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.folder-item {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.55rem 0.65rem;
		border-radius: 0.65rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 35%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
		transition:
			border-color 150ms ease,
			background 150ms ease;
	}

	.folder-item:hover {
		border-color: color-mix(in oklab, var(--brand-500) 30%, transparent);
		background: color-mix(in oklab, var(--brand-500) 4%, white);
	}

	.folder-link {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: inherit;
		text-decoration: none;
		flex: 1;
		min-width: 0;
	}

	.folder-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2rem;
		height: 2rem;
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 70%, white);
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
		flex-shrink: 0;
	}

	.folder-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}

	.folder-name {
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--base-content);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.folder-path {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.folder-menu-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.4rem;
		border: none;
		background: transparent;
		color: color-mix(in oklab, var(--base-content) 40%, transparent);
		cursor: pointer;
		flex-shrink: 0;
		transition: background 150ms ease, color 150ms ease;
	}

	.folder-menu-btn:hover {
		background: color-mix(in oklab, var(--base-300) 40%, transparent);
		color: var(--base-content);
	}

	/* Empty state */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 2.5rem 1.5rem;
		text-align: center;
		border-radius: 1rem;
		border: 2px dashed color-mix(in oklab, var(--base-300) 50%, transparent);
		background: color-mix(in oklab, var(--base-100) 96%, white);
	}

	.empty-state.minimal {
		padding: 1.5rem;
	}

	.empty-title {
		margin: 0 0 0.35rem;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--base-content);
	}

	.empty-description {
		margin: 0;
		font-size: 0.8rem;
		color: color-mix(in oklab, var(--base-content) 50%, transparent);
		max-width: 28rem;
		line-height: 1.5;
	}

	/* Responsive */
	@media (max-width: 1023px) {
		.dashboard-grid {
			grid-template-columns: 1fr;
		}

		.summary-cards {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.dashboard-sidebar {
			display: grid;
			grid-template-columns: 1fr 1fr;
			gap: 1.5rem;
		}
	}

	@media (max-width: 767px) {
		.workspace-overview-page {
			padding: 0 1rem 2rem;
			gap: 1.25rem;
		}

		.summary-cards {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: 0.625rem;
		}

		.summary-card {
			padding: 0.85rem;
		}

		.summary-value {
			font-size: 1.15rem;
		}

		.dashboard-sidebar {
			grid-template-columns: 1fr;
		}
	}

	@media (max-width: 374px) {
		.summary-cards {
			grid-template-columns: 1fr 1fr;
		}
	}
</style>
