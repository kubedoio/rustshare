<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import {
		listAdminApplications,
		enableApplication,
		disableApplication
	} from '$lib/api/admin-applications';
	import ApplicationIcon from '$lib/components/dashboard/ApplicationIcon.svelte';
	import { toastStore } from '$lib/stores/toast';
	import { ToggleLeft, ToggleRight, Edit } from 'lucide-svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';

	const queryClient = useQueryClient();

	const modulesQuery = createQuery({
		queryKey: ['admin-applications'],
		queryFn: () => listAdminApplications()
	});

	const enableMutation = createMutation({
		mutationFn: (key: string) => enableApplication(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-applications'] });
			queryClient.invalidateQueries({ queryKey: ['enabled-applications'] });
			toastStore.show('Application enabled', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	const disableMutation = createMutation({
		mutationFn: (key: string) => disableApplication(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-applications'] });
			queryClient.invalidateQueries({ queryKey: ['enabled-applications'] });
			toastStore.show('Application disabled', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	let modules = $derived($modulesQuery.data ?? []);

	let showConfirmModal = $state(false);
	let confirmTargetKey = $state('');
	let confirmTargetName = $state('');

	function handleToggle(module: (typeof modules)[0]) {
		if (module.enabled) {
			confirmTargetKey = module.application_id;
			confirmTargetName = module.display_name;
			showConfirmModal = true;
		} else {
			$enableMutation.mutate(module.application_id);
		}
	}

	function onConfirmDisable() {
		$disableMutation.mutate(confirmTargetKey);
		showConfirmModal = false;
	}
</script>

<svelte:head>
	<title>Applications - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-6xl">
	<div class="mb-6">
		<h1 class="text-2xl font-semibold text-base-content">Applications</h1>
		<p class="mt-1 text-sm text-base-content/60">
			Enable or disable workspace Applications. Disabled Applications hide from the dashboard but
			preserve all files.
		</p>
	</div>

	{#if $modulesQuery.isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else}
		<div class="overflow-x-auto rounded-2xl border border-base-300/50 bg-base-100 shadow-sm">
			<table class="table min-w-max">
				<thead>
					<tr
						class="border-b border-base-300/50 bg-base-200/30 text-left text-xs tracking-wider text-base-content/60 uppercase"
					>
						<th class="px-4 py-3 font-semibold">Application</th>
						<th class="px-4 py-3 font-semibold">Description</th>
						<th class="px-4 py-3 font-semibold">Root Path</th>
						<th class="px-4 py-3 font-semibold">Renderer</th>
						<th class="px-4 py-3 font-semibold">Document Format</th>
						<th class="px-4 py-3 font-semibold">Default Template</th>
						<th class="px-4 py-3 font-semibold">OKF</th>
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
										<ApplicationIcon name={module.icon} size={16} />
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
								{module.ui_config?.documentFormat || '-'}
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">
								{module.default_template || '-'}
							</td>
							<td class="px-4 py-3">
								{#if module.ui_config?.okf?.enabled}
									<span
										class="inline-flex items-center gap-1 rounded-full bg-info/10 px-2 py-0.5 text-[10px] font-semibold tracking-wider text-info uppercase"
									>
										OKF {module.ui_config.okf.conceptType}
									</span>
								{:else}
									<span class="text-sm text-base-content/40">-</span>
								{/if}
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
								<div class="flex items-center gap-2">
									<button
										class={`flex items-center gap-1.5 text-sm transition-colors ${module.enabled ? 'text-success' : 'text-base-content/40'}`}
										onclick={() => handleToggle(module)}
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
									<a
										href="/admin/applications/{module.application_id}/edit"
										class="btn text-base-content/50 btn-ghost btn-xs hover:text-base-content"
										title="Edit"
									>
										<Edit size={14} />
									</a>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
	<ConfirmModal
		open={showConfirmModal}
		title="Disable Application"
		message={`Disable "${confirmTargetName}"? Existing files will not be deleted.`}
		confirmLabel="Disable"
		cancelLabel="Cancel"
		danger={true}
		onConfirm={onConfirmDisable}
		onCancel={() => (showConfirmModal = false)}
	/>
</div>
