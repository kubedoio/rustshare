<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { listAdminTemplates, deleteTemplate, duplicateTemplate } from '$lib/api/admin-modules';
	import { toastStore } from '$lib/stores/toast';
	import { Plus, Trash2, Edit, Copy } from 'lucide-svelte';

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
		mutationFn: ({ key, newKey }: { key: string; newKey: string }) => duplicateTemplate(key, newKey),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			toastStore.show('Template duplicated', 'success');
		},
		onError: (err: Error) => toastStore.show(err.message, 'error')
	});

	$: templates = $templatesQuery.data ?? [];

	function handleDelete(key: string, name: string) {
		if (confirm(`Delete template "${name}"? This cannot be undone.`)) {
			$deleteMutation.mutate(key);
		}
	}

	function handleDuplicate(key: string, name: string) {
		const newKey = window.prompt(`Duplicate template "${name}". Enter a new template key:`, `${key}_copy`);
		if (!newKey) return;
		$duplicateMutation.mutate({ key, newKey });
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
						<th class="px-4 py-3 font-semibold">Module</th>
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
									<span class="text-sm font-medium text-base-content">{template.name}</span>
									<span class="text-xs text-base-content/40">{template.template_key}</span>
								</div>
							</td>
							<td class="px-4 py-3 text-sm text-base-content/70">{template.module_key}</td>
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
									>
										<Copy size={14} />
									</button>
									{#if !template.template_key.startsWith('template_default_')}
										<button
											class="btn text-error/60 btn-ghost btn-xs hover:text-error"
											title="Delete"
											on:click={() => handleDelete(template.template_key, template.name)}
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
</div>
