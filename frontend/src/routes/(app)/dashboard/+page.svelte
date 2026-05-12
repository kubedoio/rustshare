<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { listAllFiles } from '$lib/api/files';
	import { createFromTemplate } from '$lib/api/modules';
	import { createNote } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { createBrainstormBoard } from '$lib/api/brainstorming';
	import { currentUser } from '$lib/stores/auth';
	import { activityStore } from '$lib/stores/activity';
	import { filterUserVisibleEntries, isInternalRustShareFile } from '$lib/utils/artifactVisibility';
	import { getModuleObjectHref } from '$lib/modules/modulePages';
	import { getEnabledModules } from '$lib/modules/registry';
	import { formatBytes, todayDateString } from '$lib/utils/dashboard';
	import type { ModuleSummary } from '$lib/api/types';
	import type { ModuleDefinition } from '$lib/modules/registry';

	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import MetricCards from '$lib/components/dashboard/MetricCards.svelte';
	import RecentArtifacts from '$lib/components/dashboard/RecentArtifacts.svelte';
	import RecentActivity from '$lib/components/dashboard/RecentActivity.svelte';
	import QuickActions from '$lib/components/dashboard/QuickActions.svelte';
	import ModalBase from '$lib/components/common/ModalBase.svelte';
	import {
		StickyNote,
		CalendarDays,
		Columns,
		Share2,
		FileText,
		Clock,
		Package,
		HardDrive,
		Lightbulb,
		CheckCircle2,
		Folder
	} from 'lucide-svelte';

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

	let moduleRecordsCount = $derived(
		($moduleSummariesQuery.data ?? []).reduce((sum, m) => sum + (m.summary.total_items ?? 0), 0)
	);

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

	let isLoading = $derived($allFilesQuery.isLoading || $moduleSummariesQuery.isLoading);

	// ---------------------------------------------------------------------------
	// Quick actions
	// ---------------------------------------------------------------------------

	let creating = $state(false);
	let createError = $state('');
	let showNewShareModal = $state(false);

	async function handleNewNote() {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const result = await createNote({ title: 'Untitled Note', content: '# Untitled Note\n\n' });
			activityStore.addActivity('note_created', result.name || 'Untitled Note', {
				artifactId: result.id,
				moduleKey: 'notes'
			});
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
			activityStore.addActivity('meeting_created', 'Untitled Meeting Note', {
				artifactId: result.object_id,
				moduleKey: 'meetings'
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
		const content = `# Decision: ${title}\n\n## Context\n\n## Decision\n\n## Reason\n\n## Follow-up\n\n## Date\n`;
		try {
			const result = await decisionsApi.create({ title, category: 'General', content });
			activityStore.addActivity('decision_created', result.name || title || 'Untitled Decision', {
				artifactId: result.id,
				moduleKey: 'decisions'
			});
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
			activityStore.addActivity('kanban_created', 'Untitled Board', {
				artifactId: result.object_id,
				moduleKey: 'kanban'
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
			activityStore.addActivity('brainstorm_created', result.title || 'Untitled Idea Board', {
				artifactId: result.id,
				moduleKey: 'brainstorming'
			});
			goto(`/modules/brainstorming/${result.id}`);
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create idea board';
		} finally {
			creating = false;
		}
	}

	function handleNewShare() {
		showNewShareModal = true;
	}

	function handleBrowseFiles() {
		showNewShareModal = false;
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
			label: 'Files and Records',
			value: moduleRecordsCount,
			subtitle: 'Module records',
			icon: FileText,
			iconColor: '#0891b2',
			iconBg: 'rgba(8, 145, 178, 0.1)'
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
				<MetricCards cards={summaryCards} />
				<RecentArtifacts artifacts={recentArtifacts()} userName={$currentUser?.display_name} />
				<RecentActivity {activities} userName={$currentUser?.display_name} />
			</div>

			<!-- Right column -->
			<div class="dashboard-sidebar">
				<QuickActions actions={quickActions} {creating} />
			</div>
		</div>
	</div>
{/if}

<ModalBase
	open={showNewShareModal}
	title="New share"
	onClose={() => (showNewShareModal = false)}
>
	<div class="flex min-h-56 flex-col justify-between gap-6">
		<div class="flex flex-col items-center gap-3 py-6 text-center">
			<Folder size={42} class="text-brand-500" />
			<h3 class="text-base font-semibold">Choose a file or folder</h3>
			<p class="max-w-sm text-sm text-base-content/55">
				Shares are created from the Files view so the selected file or folder can be used as the source.
			</p>
		</div>
		<div class="flex justify-between">
			<button class="btn btn-sm btn-ghost" onclick={() => (showNewShareModal = false)}>Cancel</button>
			<button class="btn btn-sm btn-primary" onclick={handleBrowseFiles}>
				Open Files
			</button>
		</div>
	</div>
</ModalBase>

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
		grid-template-columns: minmax(0, 1fr) minmax(0, 320px);
		gap: 2rem;
		align-items: start;
	}

	.dashboard-main {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
		min-width: 0;
	}

	.dashboard-sidebar {
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
		min-width: 0;
	}

	/* Responsive */
	@media (max-width: 1023px) {
		.dashboard-grid {
			grid-template-columns: minmax(0, 1fr);
		}

		.dashboard-sidebar {
			display: grid;
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: 1.5rem;
		}
	}

	@media (max-width: 767px) {
		.workspace-overview-page {
			padding: 0 1rem 2rem;
			gap: 1.25rem;
		}

		.dashboard-sidebar {
			grid-template-columns: minmax(0, 1fr);
		}
	}
</style>
