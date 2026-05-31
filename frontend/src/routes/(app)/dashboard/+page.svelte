<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { queryClient } from '$lib/query-client';
	import { listAllFiles } from '$lib/api/files';
	import { listEnabledModules, createFromTemplate } from '$lib/api/modules';
	import { createNote } from '$lib/api/notes';
	import { decisionsApi } from '$lib/api/decisions';
	import { createBrainstormBoard } from '$lib/api/brainstorming';
	import { currentUser } from '$lib/stores/auth';
	import { activityStore } from '$lib/stores/activity';
	import { filterUserVisibleEntries, isInternalRustShareFile } from '$lib/utils/artifactVisibility';
	import { getModuleObjectHref, resolveModuleFolderId } from '$lib/modules/modulePages';
	import { runModulePrimaryAction } from '$lib/modules/moduleActions';
	import { todayDateString } from '$lib/utils/dashboard';
	import type { ModuleSummary, ModuleConfig } from '$lib/api/types';

	import DashboardSkeleton from '$lib/components/common/DashboardSkeleton.svelte';
	import MetricCards from '$lib/components/dashboard/MetricCards.svelte';
	import RecentActivity from '$lib/components/dashboard/RecentActivity.svelte';
	import QuickActions from '$lib/components/dashboard/QuickActions.svelte';
	import DashboardWidgetGrid from '$lib/components/dashboard/DashboardWidgetGrid.svelte';
	import PromptModal from '$lib/components/common/PromptModal.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import OfflineBanner from '$lib/components/common/OfflineBanner.svelte';
	import { Share2, Clock, Package } from 'lucide-svelte';
	import { getEnabledDashboardModules } from '$lib/modules/workspaceSurface';
	import { moduleConfigToDefinition } from '$lib/modules/registry';

	// ---------------------------------------------------------------------------
	// Types
	// ---------------------------------------------------------------------------

	interface QuickAction {
		label: string;
		subtitle: string;
		icon: unknown | string;
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

	const enabledModulesQuery = createQuery({
		queryKey: ['enabled-modules'],
		queryFn: () => listEnabledModules()
	});

	const moduleSummariesQuery = createQuery({
		queryKey: ['workspace-module-summaries'],
		queryFn: async () => {
			const { getModuleSummary } = await import('$lib/api/modules');
			const modules = await queryClient.fetchQuery({
				queryKey: ['enabled-modules'],
				queryFn: () => listEnabledModules()
			});
			const results = await Promise.all(
				modules.map(async (m) => {
					try {
						const summary: ModuleSummary = await getModuleSummary(m.module_key);
						return { module: m, summary };
					} catch {
						return null;
					}
				})
			);
			return results.filter(
				(r): r is { module: ModuleConfig; summary: ModuleSummary } => r !== null
			);
		}
	});

	// ---------------------------------------------------------------------------
	// Derived state
	// ---------------------------------------------------------------------------

	let allFiles = $derived(filterUserVisibleEntries($allFilesQuery.data ?? []));

	let updatedThisWeek = $derived(
		allFiles.filter(
			(f) => new Date(f.modified_at) >= new Date(Date.now() - 7 * 24 * 60 * 60 * 1000)
		).length
	);

	let sharedItemsCount = $derived(allFiles.filter((f) => f.is_shared).length);
	let dashboardModules = $derived(
		getEnabledDashboardModules($enabledModulesQuery.data ?? []).map(moduleConfigToDefinition)
	);

	let recentArtifacts = $derived(
		allFiles
			.toSorted((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
			.slice(0, 30)
	);

	// Build a lookup map from artifact ID → current name using data already loaded
	let nameLookup = $derived.by(() => {
		const map = new Map<string, string>();
		for (const file of allFiles) {
			map.set(file.id, file.name);
		}
		for (const { summary } of $moduleSummariesQuery.data ?? []) {
			for (const item of summary.recent_items) {
				map.set(item.id, item.name);
			}
		}
		return map;
	});

	let isLoading = $derived($allFilesQuery.isLoading || $enabledModulesQuery.isLoading);
	let isError = $derived(
		$allFilesQuery.isError || $enabledModulesQuery.isError || $moduleSummariesQuery.isError
	);
	let errorMessage = $derived(
		$allFilesQuery.error?.message ||
			$enabledModulesQuery.error?.message ||
			$moduleSummariesQuery.error?.message ||
			'Unknown error'
	);

	// ---------------------------------------------------------------------------
	// Quick actions
	// ---------------------------------------------------------------------------

	let creating = $state(false);
	let createError = $state('');
	let showDecisionModal = $state(false);
	let decisionTitle = $state('');
	let decisionTitleError = $state('');

	async function handleNewNote(rootPath?: string | null) {
		if (creating) return;
		creating = true;
		createError = '';
		try {
			const parentFolderId = rootPath ? await resolveModuleFolderId(rootPath) : null;
			const result = await createNote({
				title: 'Untitled Note',
				content: '# Untitled Note\n\n',
				parent_folder_id: parentFolderId ?? undefined
			});
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

	function handleNewMeeting() {
		goto('/modules/meetings?action=new');
	}

	function handleNewDecision() {
		decisionTitle = `Decision — ${todayDateString()}`;
		decisionTitleError = '';
		showDecisionModal = true;
	}

	async function handleDecisionConfirm(title: string) {
		if (creating) return;
		const trimmed = title.trim();
		if (!trimmed) {
			decisionTitleError = 'Title is required';
			return;
		}
		creating = true;
		createError = '';
		decisionTitleError = '';
		const content = `# Decision: ${trimmed}\n\n## Context\n\n## Decision\n\n## Reason\n\n## Follow-up\n\n## Date\n`;
		try {
			const result = await decisionsApi.create({ title: trimmed, category: 'General', content });
			activityStore.addActivity('decision_created', result.name || trimmed || 'Untitled Decision', {
				artifactId: result.id,
				moduleKey: 'decisions'
			});
			showDecisionModal = false;
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
		const boardName = `Board — ${todayDateString()}`;
		try {
			const result = await createFromTemplate({
				template_key: 'template_default_kanban',
				name: boardName,
				parent_folder_id: null
			});
			activityStore.addActivity('kanban_created', boardName, {
				artifactId: result.object_id,
				moduleKey: 'kanban'
			});
			goto('/modules/kanban');
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
			const result = await createBrainstormBoard(
				`Idea Board — ${todayDateString()}`,
				'template_blank_brainstorm'
			);
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

	const quickActions = $derived(
		($enabledModulesQuery.data ?? [])
			.filter((m) => m.enabled)
			.filter((m) => {
				const dashboard = m.ui_config?.dashboard;
				return dashboard?.enabled !== false && dashboard?.primaryAction;
			})
			.map((module): QuickAction => {
				const primaryAction = module.ui_config?.dashboard?.primaryAction;
				let handler: () => Promise<void> | void;

				switch (module.module_key) {
					case 'notes':
						handler = () => handleNewNote(module.root_path);
						break;
					case 'meetings':
						handler = handleNewMeeting;
						break;
					case 'decisions':
						handler = handleNewDecision;
						break;
					case 'kanban':
						handler = handleNewKanban;
						break;
					case 'brainstorming':
						handler = handleNewBrainstorm;
						break;
					default:
						handler = () => runModulePrimaryAction(module, primaryAction);
						break;
				}

				return {
					label: primaryAction?.label ?? `New ${module.display_name.toLowerCase()}`,
					subtitle: module.description,
					icon: module.icon,
					iconColor: '#ea580c',
					iconBg: 'rgba(234, 88, 12, 0.1)',
					onClick: handler
				};
			})
	);

	const summaryCards = $derived([
		{
			label: 'Recent Artifacts',
			value: recentArtifacts.length,
			subtitle: 'Last 30 items',
			icon: Package,
			iconColor: '#ea580c',
			iconBg: 'rgba(234, 88, 12, 0.1)',
			href: '/files?filter=recent'
		},
		{
			label: 'Updated This Week',
			value: updatedThisWeek,
			subtitle: 'This week',
			icon: Clock,
			iconColor: '#16a34a',
			iconBg: 'rgba(22, 163, 74, 0.1)',
			href: '/files?filter=week'
		},
		{
			label: 'Shared Items',
			value: sharedItemsCount,
			subtitle: 'Active shares',
			icon: Share2,
			iconColor: '#7c3aed',
			iconBg: 'rgba(124, 58, 237, 0.1)',
			href: '/files?root=shared'
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
				<DashboardWidgetGrid modules={dashboardModules} />
				<RecentActivity userName={$currentUser?.display_name} />
			</div>

			<!-- Right column -->
			<div class="dashboard-sidebar">
				<QuickActions actions={quickActions} {creating} />
			</div>
		</div>
	</div>
{/if}

<PromptModal
	open={showDecisionModal}
	title="New decision record"
	message="Enter a title for the new decision record."
	defaultValue={decisionTitle}
	confirmLabel="Create"
	error={decisionTitleError}
	isLoading={creating}
	onConfirm={handleDecisionConfirm}
	onCancel={() => {
		showDecisionModal = false;
		decisionTitleError = '';
	}}
/>

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
