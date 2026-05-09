<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listAllFiles } from '$lib/api/files';
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
		Zap
	} from 'lucide-svelte';
	import { getModuleObjectHref } from '$lib/modules/modulePages';
	import { getEnabledModules } from '$lib/modules/registry';
	import type { ModuleSummary } from '$lib/api/types';
	import type { ModuleDefinition } from '$lib/modules/registry';
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
		icon: any;
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
			meetings: 'Meeting',
			standups: 'Standup',
			kanban: 'Board',
			decisions: 'Decision',
			brainstorming: 'Idea board',
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

	// ---------------------------------------------------------------------------
	// Queries
	// ---------------------------------------------------------------------------

	const allFilesQuery = createQuery({
		queryKey: ['all-files'],
		queryFn: () => listAllFiles()
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
		{ label: 'New note', icon: StickyNote, onClick: handleNewNote },
		{ label: 'New meeting note', icon: CalendarDays, onClick: handleNewMeeting },
		{ label: 'New decision', icon: GitBranch, onClick: handleNewDecision },
		{ label: 'New board', icon: Columns, onClick: handleNewKanban },
		{ label: 'New idea board', icon: PenTool, onClick: handleNewBrainstorm },
		{ label: 'New share', icon: Share2, onClick: handleNewShare }
	];
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

		<!-- Summary cards -->
		<section class="summary-cards" aria-label="Workspace summary">
			<div class="summary-card">
				<div class="summary-icon">
					<Package size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{totalArtifacts}</span>
					<span class="summary-label">Total artifacts</span>
				</div>
			</div>
			<div class="summary-card">
				<div class="summary-icon">
					<Zap size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{updatedThisWeek()}</span>
					<span class="summary-label">Updated this week</span>
				</div>
			</div>
			<div class="summary-card">
				<div class="summary-icon">
					<Share2 size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{sharedItemsCount}</span>
					<span class="summary-label">Shared items</span>
				</div>
			</div>
			<div class="summary-card">
				<div class="summary-icon">
					<HardDrive size={18} />
				</div>
				<div class="summary-body">
					<span class="summary-value">{storageUsed}</span>
					<span class="summary-label">Storage used</span>
				</div>
			</div>
		</section>

		<!-- Quick actions -->
		<section class="quick-actions" aria-label="Quick actions">
			<h2 class="section-title">Quick actions</h2>
			<div class="action-grid">
				{#each quickActions as action}
					<button
						type="button"
						class="action-button"
						onclick={action.onClick}
						disabled={creating}
					>
						<svelte:component this={action.icon} size={18} />
						<span>{action.label}</span>
					</button>
				{/each}
			</div>
		</section>

		<!-- Recent artifacts -->
		<section class="recent-artifacts" aria-label="Recent artifacts">
			<h2 class="section-title">Recent artifacts</h2>
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
						<li>
							<a href={getArtifactHref(item)} class="artifact-link">
								<div class="artifact-icon">
									{#if item.item_type === 'folder'}
										<Folder size={14} />
									{:else}
										<FileText size={14} />
									{/if}
								</div>
								<div class="artifact-body">
									<span class="artifact-name">{cleanArtifactName(item.name)}</span>
									<div class="artifact-meta">
										<span class="artifact-type">{getArtifactTypeLabel(item.moduleKey, item.item_type)}</span>
										<span class="artifact-time">
											<Clock size={10} />
											{formatDistanceToNow(new Date(item.updated_at), { addSuffix: true })}
										</span>
									</div>
								</div>
								<ArrowUpRight size={14} class="artifact-arrow" />
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<!-- Recent activity -->
		<section class="recent-activity" aria-label="Recent activity">
			<h2 class="section-title">Recent activity</h2>
			{#if activities.length === 0}
				<div class="empty-state minimal">
					<p class="empty-description">Activity will appear here as you work in your workspace.</p>
				</div>
			{:else}
				<ul class="activity-list">
					{#each activities as activity}
						{@const display = getActivityDisplay(activity)}
						<li class="activity-item">
							<span class="activity-icon">{display.icon}</span>
							<div class="activity-body">
								<span class="activity-text">{display.description}</span>
								<span class="activity-time">{getRelativeTime(activity.timestamp)}</span>
							</div>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
{/if}

<style>
	.workspace-overview-page {
		max-width: 1100px;
		margin: 0 auto;
		padding: 0 2rem 3rem;
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	.overview-header {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding-top: 0.5rem;
	}

	.overview-header h1 {
		margin: 0;
		font-size: clamp(1.5rem, 3vw, 2rem);
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

	.section-title {
		margin: 0 0 0.75rem;
		font-size: 0.85rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
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
		width: 2.25rem;
		height: 2.25rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--brand-500) 10%, transparent);
		color: var(--brand-500);
		flex-shrink: 0;
	}

	.summary-body {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		min-width: 0;
	}

	.summary-value {
		font-size: 1.1rem;
		font-weight: 700;
		color: var(--base-content);
		line-height: 1.2;
	}

	.summary-label {
		font-size: 0.72rem;
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
	}

	/* Quick actions */
	.action-grid {
		display: grid;
		grid-template-columns: repeat(6, minmax(0, 1fr));
		gap: 0.625rem;
	}

	.action-button {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.4rem;
		padding: 0.85rem 0.5rem;
		border-radius: 0.875rem;
		border: 1px solid color-mix(in oklab, var(--base-300) 45%, transparent);
		background: color-mix(in oklab, var(--base-100) 94%, white);
		color: var(--base-content);
		font-size: 0.72rem;
		font-weight: 600;
		cursor: pointer;
		transition:
			background 150ms ease,
			border-color 150ms ease,
			color 150ms ease;
		box-shadow: 0 4px 16px rgb(72 42 17 / 0.04);
	}

	.action-button:hover:not(:disabled) {
		border-color: var(--brand-500);
		color: var(--brand-500);
		background: color-mix(in oklab, var(--brand-500) 5%, white);
	}

	.action-button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Recent artifacts */
	.artifact-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}

	.artifact-link {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 0.85rem;
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
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.5rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 70%, white);
		color: color-mix(in oklab, var(--base-content) 55%, transparent);
		flex-shrink: 0;
	}

	.artifact-body {
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
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

	.artifact-type {
		font-weight: 600;
	}

	.artifact-time {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
	}

	.artifact-arrow {
		color: color-mix(in oklab, var(--base-content) 30%, transparent);
		flex-shrink: 0;
		transition: color 150ms ease;
	}

	.artifact-link:hover .artifact-arrow {
		color: var(--brand-500);
	}

	/* Activity */
	.activity-list {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.activity-item {
		display: flex;
		align-items: flex-start;
		gap: 0.6rem;
		padding: 0.5rem 0.75rem;
		border-radius: 0.65rem;
		background: color-mix(in oklab, var(--rs-surface-muted) 50%, white);
	}

	.activity-icon {
		font-size: 0.95rem;
		line-height: 1;
		margin-top: 0.1rem;
	}

	.activity-body {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		min-width: 0;
		flex: 1;
	}

	.activity-text {
		font-size: 0.8rem;
		color: var(--base-content);
	}

	.activity-time {
		font-size: 0.7rem;
		color: color-mix(in oklab, var(--base-content) 45%, transparent);
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
		.summary-cards {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.action-grid {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}
	}

	@media (max-width: 767px) {
		.workspace-overview-page {
			padding: 0 1rem 2rem;
			gap: 1.5rem;
		}

		.summary-cards {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: 0.625rem;
		}

		.summary-card {
			padding: 0.85rem;
		}

		.summary-value {
			font-size: 1rem;
		}

		.action-grid {
			grid-template-columns: repeat(3, minmax(0, 1fr));
			gap: 0.5rem;
		}

		.action-button {
			padding: 0.7rem 0.3rem;
			font-size: 0.68rem;
		}
	}

	@media (max-width: 374px) {
		.summary-cards {
			grid-template-columns: 1fr 1fr;
		}

		.action-grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
