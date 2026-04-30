<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import { getModule } from '$lib/api/modules';
	import { ApiError } from '$lib/api/types';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import { getModulePageConfig } from '$lib/modules/workspaceSurface';
	import NotesModuleView from '$lib/components/modules/NotesModuleView.svelte';
	import KanbanModuleView from '$lib/components/modules/KanbanModuleView.svelte';
	import MeetingsModuleView from '$lib/components/modules/MeetingsModuleView.svelte';
	import StandupsModuleView from '$lib/components/modules/StandupsModuleView.svelte';
	import DecisionsModuleView from '$lib/components/modules/DecisionsModuleView.svelte';
	import SharesModuleView from '$lib/components/modules/SharesModuleView.svelte';
	import GenericModuleView from '$lib/components/modules/GenericModuleView.svelte';

	const pageRendererRegistry: Record<string, any> = {
		notes: NotesModuleView,
		kanban: KanbanModuleView,
		meetings: MeetingsModuleView,
		standups: StandupsModuleView,
		decisions: DecisionsModuleView,
		shares: SharesModuleView
	};
	import { ArrowLeft, AlertCircle } from 'lucide-svelte';

	$: moduleKey = $page.params.key ?? '';

	const moduleQuery = createQuery({
		queryKey: ['module', moduleKey],
		queryFn: () => getModule(moduleKey)
	});

	$: moduleConfig = $moduleQuery.data;
	$: modulePageConfig = moduleConfig ? getModulePageConfig(moduleConfig) : null;
	$: moduleError = $moduleQuery.error;
	$: isLoading = $moduleQuery.isLoading;
	$: errorStatus = moduleError instanceof ApiError ? moduleError.status : null;
	$: missingModule = errorStatus === 404;
	$: accessDenied = errorStatus === 403;
	$: pageDisabled = modulePageConfig?.enabled === false;
</script>

<svelte:head>
	<title>{moduleConfig?.display_name || 'Module'} - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-5xl p-4 lg:p-6">
	<a
		href="/dashboard"
		class="mb-4 inline-flex items-center gap-1.5 text-sm text-base-content/50 transition-colors hover:text-base-content"
	>
		<ArrowLeft size={14} />
		Back to Dashboard
	</a>

	{#if isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else if missingModule || accessDenied || pageDisabled}
		<div
			class="flex flex-col items-center justify-center gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-12 text-center"
		>
			<div
				class="flex h-16 w-16 items-center justify-center rounded-full bg-base-200 text-base-content/30"
			>
				<AlertCircle size={32} />
			</div>
			<h1 class="text-xl font-semibold text-base-content">
				{missingModule
					? 'Module Not Found'
					: pageDisabled
						? 'Module Disabled'
						: 'Module Not Available'}
			</h1>
			<p class="max-w-sm text-sm text-base-content/60">
				{#if missingModule}
					No module is registered for <code>{moduleKey}</code>.
				{:else if pageDisabled}
					This module page is disabled by your workspace administrator.
				{:else}
					{moduleError?.message ?? 'This module is disabled or you do not have access to it.'}
				{/if}
			</p>
			<a href="/dashboard" class="btn btn-sm btn-primary">Back to Dashboard</a>
		</div>
	{:else}
		<div class="flex flex-col gap-6">
			<!-- Module Header -->
			<div class="flex items-start gap-4 rounded-2xl border border-base-300/50 bg-base-100 p-6">
				<div
					class="flex h-12 w-12 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
				>
					<ModuleIcon name={moduleConfig?.icon ?? 'grid'} size={24} />
				</div>
				<div class="flex flex-col gap-1">
					<h1 class="text-lg font-semibold text-base-content">
						{moduleConfig?.display_name ?? 'Module'}
					</h1>
					<p class="text-sm text-base-content/60">{moduleConfig?.description ?? ''}</p>
					<div class="mt-1 flex items-center gap-2">
						<span
							class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/50 uppercase"
						>
							{moduleConfig?.root_path ?? '/'}
						</span>
						<span
							class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/50 uppercase"
						>
							{modulePageConfig?.renderer ?? moduleConfig?.renderer ?? 'default'}
						</span>
					</div>
				</div>
			</div>

			<!-- Module Contents -->
			{#if moduleConfig}
				{@const Renderer =
					pageRendererRegistry[modulePageConfig?.renderer ?? moduleConfig.renderer] ??
					GenericModuleView}
				<Renderer {moduleConfig} />
			{/if}
		</div>
	{/if}
</div>
