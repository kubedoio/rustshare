<script lang="ts">
	import type { ModuleConfig } from '$lib/api/types';
	import ModuleIcon from './ModuleIcon.svelte';
	import { ArrowRight } from 'lucide-svelte';

	export let module: ModuleConfig;

	function getActionLabel(renderer: string): string {
		const labels: Record<string, string> = {
			notes: 'Open Notes',
			'meeting-notes': 'New Meeting Note',
			standups: "Today's Standup",
			kanban: 'Open Boards',
			decisions: 'New Decision',
			shares: 'Manage Shares'
		};
		return labels[renderer] || 'Open';
	}
</script>

<a
	href="/modules/{module.module_key}"
	class="group flex flex-col gap-3 rounded-2xl border border-base-300/50 bg-base-100 p-5 shadow-sm transition-all duration-200 hover:border-brand-500/40 hover:shadow-md"
>
	<div class="flex items-start justify-between">
		<div
			class="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500 transition-colors group-hover:bg-brand-500 group-hover:text-white"
		>
			<ModuleIcon name={module.icon} size={20} />
		</div>
		<span
			class="rounded-full border border-base-300/60 bg-base-200/50 px-2.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-base-content/50"
		>
			{module.root_path}
		</span>
	</div>

	<div class="flex flex-col gap-1">
		<h3 class="text-sm font-semibold text-base-content">{module.display_name}</h3>
		<p class="text-xs leading-relaxed text-base-content/60">{module.description}</p>
	</div>

	<div class="mt-auto pt-1">
		<span
			class="inline-flex items-center gap-1.5 rounded-lg bg-brand-500/5 px-3 py-1.5 text-xs font-medium text-brand-600 transition-colors group-hover:bg-brand-500/10"
		>
			{getActionLabel(module.renderer)}
			<ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
		</span>
	</div>
</a>
