<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import {
		listAdminTemplates,
		deleteTemplate,
		duplicateTemplate
	} from '$lib/api/admin-applications';
	import { toastStore } from '$lib/stores/toast';
	import { Plus, Trash2, Edit, Copy } from 'lucide-svelte';
	import ConfirmModal from '$lib/components/common/ConfirmModal.svelte';

	const queryClient = useQueryClient();

	const templatesQuery = createQuery({
		queryKey: ['admin-templates'],
		queryFn: () => listAdminTemplates()
	});

	const deleteMutation = createMutation({
		mutationFn: (key: string) => deleteTemplate(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			toastStore.show('Template deleted', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	const duplicateMutation = createMutation({
		mutationFn: (key: string) => duplicateTemplate(key),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			toastStore.show('Template duplicated', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	let templates = $derived($templatesQuery.data ?? []);

	let showConfirmModal = $state(false);
	let confirmTargetKey = $state('');
	let confirmTargetName = $state('');

	function handleDelete(key: string, name: string) {
		confirmTargetKey = key;
		confirmTargetName = name;
		showConfirmModal = true;
	}

	function onConfirmDelete() {
		$deleteMutation.mutate(confirmTargetKey);
		showConfirmModal = false;
	}

	function handleDuplicate(key: string) {
		$duplicateMutation.mutate(key);
	}

	function formatDate(dateStr: string | null): string {
		if (!dateStr) return '-';
		return new Date(dateStr).toLocaleDateString();
	}
</script>

<svelte:head>
	<title>Templates - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-6xl">
	<div class="mb-6 flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-semibold text-base-content">Templates</h1>
			<p class="mt-1 text-sm text-base-content/60">
				Manage predefined and custom templates for workspace modules.
			</p>
		</div>
		<a href="/admin/templates/new" class="btn btn-sm btn-primary">
			<Plus size={14} />
			<span>New Template</span>
		</a>
	</div>

	{#if $templatesQuery.isLoading}
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
						<th class="px-4 py-3 font-semibold">Template</th>
						<th class="px-4 py-3 font-semibold">Application</th>
						<th class="px-4 py-3 font-semibold">Renderer</th>
						<th class="px-4 py-3 font-semibold">Version</th>
						<th class="px-4 py-3 font-semibold">Created</th>
						<th class="px-4 py-3 font-semibold">Enabled</th>
						<th class="px-4 py-3 text-right font-semibold">Actions</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-base-300/30">
					{#each templates as template}
						<tr class="transition-colors hover:bg-base-200/20">
							<td class="px-4 py-3">
								<div class="flex flex-col">
									<div class="flex items-center gap-2">
										<span class="text-sm font-medium text-base-content">{template.name}</span>
										{#if template.system_template}
											<span
												class="rounded bg-base-300/50 px-1 py-0.5 text-[8px] font-bold tracking-tight text-base-content/60 uppercase"
											>
												System
											</span>
										{/if}
									</div>
									<span class="text-xs text-base-content/40">{template.template_key}</span>
								</div>
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">{template.application_id}</td>
							<td class="px-4 py-3 text-sm text-base-content/70">
								{template.renderer || '-'}
								{#if template.template_key === 'template_default_okf_note'}
									<span
										class="ml-2 rounded bg-info/10 px-1 py-0.5 text-[8px] font-bold tracking-tight text-info uppercase"
									>
										OKF
									</span>
								{/if}
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">{template.version}</td>
							<td class="px-4 py-3 text-sm text-base-content/70">
								{formatDate(template.created_at)}
							</td>
							<td class="px-4 py-3">
								{#if template.enabled}
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
							<td class="px-4 py-3 text-right">
								<div class="flex items-center justify-end gap-1">
									<a
										href="/admin/templates/{template.template_key}/edit"
										class="btn text-base-content/50 btn-ghost btn-xs hover:text-base-content"
										title="Edit"
									>
										<Edit size={14} />
									</a>
									<button
										class="btn text-base-content/50 btn-ghost btn-xs hover:text-base-content"
										title="Duplicate"
										onclick={() => handleDuplicate(template.template_key)}
										disabled={$duplicateMutation.isPending}
									>
										<Copy size={14} />
									</button>
									{#if !template.system_template}
										<button
											class="btn text-error/60 btn-ghost btn-xs hover:text-error"
											title="Delete"
											onclick={() => handleDelete(template.template_key, template.name)}
											disabled={$deleteMutation.isPending}
										>
											<Trash2 size={14} />
										</button>
									{/if}
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
		title="Delete Template"
		message={`Delete template "${confirmTargetName}"? This cannot be undone.`}
		confirmLabel="Delete"
		cancelLabel="Cancel"
		danger={true}
		onConfirm={onConfirmDelete}
		onCancel={() => (showConfirmModal = false)}
	/>
</div>
