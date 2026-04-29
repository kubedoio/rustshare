<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { listAdminModules, enableModule, disableModule } from '$lib/api/admin-modules';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { ToggleLeft, ToggleRight } from 'lucide-svelte';

	const queryClient = useQueryClient();

	const modulesQuery = createQuery({
		queryKey: ['admin-modules'],
		queryFn: () => listAdminModules()
	});

	const enableMutation = createMutation({
		mutationFn: (key: string) => enableModule(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-modules'] });
			queryClient.invalidateQueries({ queryKey: ['enabled-modules'] });
			toastStore.show('Module enabled', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	const disableMutation = createMutation({
		mutationFn: (key: string) => disableModule(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-modules'] });
			queryClient.invalidateQueries({ queryKey: ['enabled-modules'] });
			toastStore.show('Module disabled', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	$: modules = $modulesQuery.data ?? [];

	function handleToggle(module: (typeof modules)[0]) {
		if (module.enabled) {
			if (confirm(`Disable "${module.display_name}"? Existing files will not be deleted.`)) {
				$disableMutation.mutate(module.module_key);
			}
		} else {
			$enableMutation.mutate(module.module_key);
		}
	}
</script>

<svelte:head>
	<title>Modules - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-6xl">
	<div class="mb-6">
		<h1 class="text-2xl font-semibold text-base-content">Modules</h1>
		<p class="mt-1 text-sm text-base-content/60">
			Enable or disable workspace modules. Disabled modules hide from the dashboard but preserve all
			files.
		</p>
	</div>

	{#if $modulesQuery.isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else}
		<div class="overflow-hidden rounded-2xl border border-base-300/50 bg-base-100 shadow-sm">
			<table class="table w-full">
				<thead>
					<tr
						class="border-b border-base-300/50 bg-base-200/30 text-left text-xs tracking-wider text-base-content/60 uppercase"
					>
						<th class="px-4 py-3 font-semibold">Module</th>
						<th class="px-4 py-3 font-semibold">Description</th>
						<th class="px-4 py-3 font-semibold">Root Path</th>
						<th class="px-4 py-3 font-semibold">Renderer</th>
						<th class="px-4 py-3 font-semibold">Default Template</th>
						<th class="px-4 py-3 font-semibold">Enabled</th>
						<th class="px-4 py-3 font-semibold">Actions</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-base-300/30">
					{#each modules as module}
						<tr class="transition-colors hover:bg-base-200/20">
							<td class="px-4 py-3">
								<div class="flex items-center gap-3">
									<div
										class="flex h-8 w-8 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500"
									>
										<ModuleIcon name={module.icon} size={16} />
									</div>
									<span class="text-sm font-medium text-base-content">
										{module.display_name}
									</span>
								</div>
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">{module.description}</td>
							<td class="px-4 py-3">
								<span
									class="rounded-full border border-base-300/60 bg-base-200/50 px-2 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/50 uppercase"
								>
									{module.root_path}
								</span>
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">{module.renderer}</td>
							<td class="px-4 py-3 text-sm text-base-content/70">
								{module.default_template || '-'}
							</td>
							<td class="px-4 py-3">
								{#if module.enabled}
									<span
										class="inline-flex items-center gap-1 rounded-full bg-success/10 px-2 py-0.5 text-[10px] font-semibold tracking-wider text-success uppercase"
									>
										Active
									</span>
								{:else}
									<span
										class="inline-flex items-center gap-1 rounded-full bg-base-300/40 px-2 py-0.5 text-[10px] font-semibold tracking-wider text-base-content/40 uppercase"
									>
										Disabled
									</span>
								{/if}
							</td>
							<td class="px-4 py-3">
								<button
									class={`flex items-center gap-1.5 text-sm transition-colors ${module.enabled ? 'text-success' : 'text-base-content/40'}`}
									on:click={() => handleToggle(module)}
									disabled={$enableMutation.isPending || $disableMutation.isPending}
								>
									{#if module.enabled}
										<ToggleRight size={20} />
									{:else}
										<ToggleLeft size={20} />
									{/if}
									<span class="text-xs font-medium">
										{module.enabled ? 'On' : 'Off'}
									</span>
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>
