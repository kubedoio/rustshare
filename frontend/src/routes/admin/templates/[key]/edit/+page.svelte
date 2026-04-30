<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import { getAdminTemplate, updateTemplate, listAdminModules } from '$lib/api/admin-modules';
	import { toastStore } from '$lib/stores/toast';
	import { ArrowLeft, Save, AlertCircle } from 'lucide-svelte';

	const queryClient = useQueryClient();
	const key = $page.params.key!;

	let name = '';
	let moduleKey = '';
	let description = '';
	let folderStructureJson = '[]';
	let defaultFilesJson = '[]';
	let metadataSchemaJson = '{}';
	let renderer = '';
	let visibilityPolicy = 'workspace';
	let enabled = true;
	let error = '';

	const templateQuery = createQuery({
		queryKey: ['admin-template', key],
		queryFn: () => getAdminTemplate(key)
	});

	const modulesQuery = createQuery({
		queryKey: ['admin-modules'],
		queryFn: () => listAdminModules()
	});

	$: if ($templateQuery.data) {
		const t = $templateQuery.data;
		name = t.name;
		moduleKey = t.module_key;
		description = t.description ?? '';
		folderStructureJson = JSON.stringify(t.folder_structure ?? [], null, 2);
		defaultFilesJson = JSON.stringify(t.default_files ?? [], null, 2);
		metadataSchemaJson = JSON.stringify(t.metadata_schema ?? {}, null, 2);
		renderer = t.renderer ?? '';
		visibilityPolicy = t.visibility_policy ?? 'workspace';
		enabled = t.enabled ?? true;
	}

	const updateMutation = createMutation({
		mutationFn: (payload: {
			name: string;
			module_key: string;
			description: string;
			folder_structure: string[];
			default_files: { path: string; content?: string; content_type?: string }[];
			metadata_schema: Record<string, unknown>;
			renderer: string | null;
			visibility_policy: string;
			enabled: boolean;
		}) => updateTemplate(key, payload),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			queryClient.invalidateQueries({ queryKey: ['admin-template', key] });
			toastStore.show('Template updated', 'success');
			goto('/admin/templates');
		},
		onError: (err: Error) => {
			toastStore.show(err.message, 'error');
		}
	});

	function validateJson(label: string, value: string): unknown {
		try {
			return JSON.parse(value);
		} catch (e) {
			throw new Error(`Invalid JSON in ${label}: ${e instanceof Error ? e.message : String(e)}`, {
				cause: e
			});
		}
	}

	function handleSubmit() {
		error = '';

		if (!name.trim() || !moduleKey.trim()) {
			error = 'Template name and module are required.';
			return;
		}

		try {
			const folderStructure = validateJson('Folder Structure', folderStructureJson) as string[];
			const defaultFiles = validateJson('Default Files', defaultFilesJson);
			const metadataSchema = validateJson('Metadata Schema', metadataSchemaJson);

			$updateMutation.mutate({
				name: name.trim(),
				module_key: moduleKey,
				description: description.trim(),
				folder_structure: Array.isArray(folderStructure) ? folderStructure : [],
				default_files: Array.isArray(defaultFiles) ? defaultFiles : [],
				metadata_schema: metadataSchema as Record<string, unknown>,
				renderer: renderer.trim() || null,
				visibility_policy: visibilityPolicy,
				enabled
			});
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}
</script>

<svelte:head>
	<title>Edit Template - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-3xl">
	<a
		href="/admin/templates"
		class="mb-4 inline-flex items-center gap-1.5 text-sm text-base-content/50 transition-colors hover:text-base-content"
	>
		<ArrowLeft size={14} />
		Back to Templates
	</a>

	<h1 class="text-2xl font-semibold text-base-content">Edit Template</h1>
	<p class="mt-1 text-sm text-base-content/60">Update template settings and structure.</p>

	{#if $templateQuery.isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else if $templateQuery.isError}
		<div
			class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error"
		>
			<AlertCircle size={16} />
			Failed to load template: {$templateQuery.error?.message ?? 'Unknown error'}
		</div>
	{:else}
		{#if error}
			<div
				class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error"
			>
				<AlertCircle size={16} />
				{error}
			</div>
		{/if}

		<form on:submit|preventDefault={handleSubmit} class="mt-6 flex flex-col gap-4">
			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Basic Information
				</h2>
				<div class="grid gap-4">
					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="template-key"
								>Template Key</label
							>
							<input
								id="template-key"
								type="text"
								class="input-bordered input input-sm bg-base-200/50"
								value={key}
								disabled
							/>
							<p class="text-[10px] text-base-content/40">Template key cannot be changed.</p>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="template-name"
								>Template Name *</label
							>
							<input
								id="template-name"
								type="text"
								class="input-bordered input input-sm"
								placeholder="My Custom Template"
								bind:value={name}
								required
							/>
						</div>
					</div>

					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="module">Module *</label>
						<select
							id="module"
							class="select-bordered select select-sm"
							bind:value={moduleKey}
							required
						>
							<option value="" disabled>Select a module</option>
							{#each $modulesQuery.data ?? [] as mod}
								<option value={mod.module_key}>{mod.display_name}</option>
							{/each}
						</select>
					</div>

					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="description"
							>Description</label
						>
						<textarea
							id="description"
							class="textarea-bordered textarea textarea-sm"
							placeholder="What this template creates..."
							bind:value={description}
							rows={2}
						></textarea>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="renderer"
								>Renderer</label
							>
							<input
								id="renderer"
								type="text"
								class="input-bordered input input-sm"
								placeholder="e.g. notes, kanban"
								bind:value={renderer}
							/>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="visibility-policy"
								>Visibility Policy</label
							>
							<select
								id="visibility-policy"
								class="select-bordered select select-sm"
								bind:value={visibilityPolicy}
							>
								<option value="workspace">Workspace</option>
								<option value="admin-only">Admin Only</option>
								<option value="public">Public</option>
							</select>
						</div>
					</div>

					<div class="flex items-center gap-3">
						<input
							type="checkbox"
							class="checkbox checkbox-sm"
							bind:checked={enabled}
							id="enabled"
						/>
						<label for="enabled" class="text-sm text-base-content/80">Enabled</label>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Structure & Content
				</h2>
				<div class="flex flex-col gap-4">
					<div class="flex flex-col gap-1">
						<label
							class="text-xs font-semibold text-base-content/70"
							for="folder-structure-json-array">Folder Structure (JSON array)</label
						>
						<textarea
							id="folder-structure-json-array"
							class="textarea-bordered textarea font-mono text-xs textarea-sm"
							bind:value={folderStructureJson}
							rows={4}
						></textarea>
					</div>

					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="default-files-json-array"
							>Default Files (JSON array)</label
						>
						<textarea
							id="default-files-json-array"
							class="textarea-bordered textarea font-mono text-xs textarea-sm"
							bind:value={defaultFilesJson}
							rows={6}
						></textarea>
					</div>

					<div class="flex flex-col gap-1">
						<label
							class="text-xs font-semibold text-base-content/70"
							for="metadata-schema-json-object">Metadata Schema (JSON object)</label
						>
						<textarea
							id="metadata-schema-json-object"
							class="textarea-bordered textarea font-mono text-xs textarea-sm"
							bind:value={metadataSchemaJson}
							rows={4}
						></textarea>
					</div>
				</div>
			</div>

			<div class="flex items-center justify-end gap-3">
				<a href="/admin/templates" class="btn btn-ghost btn-sm">Cancel</a>
				<button type="submit" class="btn btn-sm btn-primary" disabled={$updateMutation.isPending}>
					{#if $updateMutation.isPending}
						<span class="loading loading-xs loading-spinner"></span>
					{:else}
						<Save size={14} />
					{/if}
					<span>Save Changes</span>
				</button>
			</div>
		</form>
	{/if}
</div>
