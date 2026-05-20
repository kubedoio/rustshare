<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import ModuleIcon from './ModuleIcon.svelte';
	import { ArrowRight, FileText, Folder } from 'lucide-svelte';
	import { createQuery } from '$lib/query-compat';
	import { getModuleSummary } from '$lib/api/modules';
	import { runModulePrimaryAction } from '$lib/modules/moduleActions';
	import { filterUserVisibleEntries } from '$lib/utils/artifactVisibility';

	let {
		module
	}: {
		module: ModuleConfig;
	} = $props();

	let cardTitle = $derived(module.ui_config?.dashboard?.cardTitle ?? module.display_name);
	let cardDescription = $derived(module.ui_config?.dashboard?.cardDescription ?? module.description);
	let actionLabel = $derived(module.ui_config?.dashboard?.primaryAction?.label ?? 'Open');
	let summaryMode = $derived(module.ui_config?.dashboard?.summaryMode ?? 'none');
	let maxItems = $derived(module.ui_config?.dashboard?.maxItems ?? 4);

	import { get } from 'svelte/store';

	const summaryQuery = createQuery({
		queryKey: ['module-summary', module.module_key],
		queryFn: () => getModuleSummary(module.module_key),
		enabled: get($derived(summaryMode !== 'none'))
	});

	let summary = $derived($summaryQuery.data);
	let hasSummary = $derived(summaryMode !== 'none' && summary && !$summaryQuery.isLoading);
	let visibleItems = $derived(hasSummary ? filterUserVisibleEntries(summary!.recent_items).slice(0, maxItems) : []);
	let sharesExtra = $derived((summary?.extra ?? {}) as { publicCount?: number; internalCount?: number });
	let standupsExtra = $derived((summary?.extra ?? {}) as { todayExists?: boolean });
	let kanbanExtra = $derived((summary?.extra ?? {}) as { boards?: Array<{ name: string }> });

	async function handlePrimaryAction() {
		await runModulePrimaryAction(module, module.ui_config?.dashboard?.primaryAction);
	}
</script>

<div
	class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 shadow-sm transition-all duration-200 hover:border-brand-500/40 hover:shadow-md"
>
	<a
		href="/modules/{module.module_key}"
		class="flex flex-col gap-3"
		aria-label={'Open ' + cardTitle + ' module'}
	>
		<div class="flex items-start justify-between">
			<div
				class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500 transition-colors group-hover:bg-brand-500 group-hover:text-white"
			>
				<ModuleIcon name={module.icon} size={20} />
			</div>
			<span
				class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/50 uppercase"
			>
				{module.root_path}
			</span>
		</div>

		<div class="flex flex-col gap-1">
			<h3 class="text-sm font-semibold text-base-content">{cardTitle}</h3>
			{#if hasSummary}
				{#if summary!.total_items > 0}
					<p class="text-xs leading-relaxed text-base-content/60">
						{summary!.total_items} item{summary!.total_items === 1 ? '' : 's'}
					</p>
					{#if summaryMode === 'shares-overview'}
						<p class="mt-1 text-xs text-base-content/50">
							{sharesExtra.publicCount ?? 0} public, {sharesExtra.internalCount ?? 0} internal
						</p>
					{:else if summaryMode === 'today-status'}
						<p class="mt-1 text-xs text-base-content/50">
							{standupsExtra.todayExists
								? "Today's standup is recorded"
								: "Today's standup is still pending"}
						</p>
					{:else if summaryMode === 'kanban-overview' && (kanbanExtra.boards?.length ?? 0) > 0}
						<p class="mt-1 text-xs text-base-content/50">
							{kanbanExtra.boards!.length} active board{kanbanExtra.boards!.length === 1 ? '' : 's'}
						</p>
					{/if}
					{#if visibleItems.length > 0}
						<ul class="mt-1 flex flex-col gap-0.5">
							{#each visibleItems as item}
								<li class="flex items-center gap-1.5 text-xs text-base-content/50">
									{#if item.item_type === 'file'}
										<FileText size={12} />
									{:else}
										<Folder size={12} />
									{/if}
									<span class="truncate">{item.name}</span>
								</li>
							{/each}
						</ul>
					{/if}
				{:else}
					<p class="text-xs leading-relaxed text-base-content/40">No items yet</p>
				{/if}
			{:else}
				<p class="text-xs leading-relaxed text-base-content/60">{cardDescription}</p>
			{/if}
		</div>
	</a>

	<div class="mt-auto pt-1">
		<button
			type="button"
			class="inline-flex items-center gap-1.5 rounded-lg bg-brand-500/5 px-3 py-1.5 text-xs font-medium text-brand-600 transition-colors hover:bg-brand-500/10 focus:ring-2 focus:ring-brand-500/40 focus:outline-none"
			onclick={handlePrimaryAction}
			aria-label={actionLabel + ' in ' + module.display_name}
		>
			{actionLabel}
			<ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
		</button>
	</div>
</div>
