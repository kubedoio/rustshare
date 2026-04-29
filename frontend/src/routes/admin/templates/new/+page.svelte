<script lang="ts">
	import { goto } from '$app/navigation';
	import { createMutation, useQueryClient } from '$lib/query-compat';
	import { createTemplate } from '$lib/api/admin-modules';
	import { listAdminModules } from '$lib/api/admin-modules';
	import { toast } from '$lib/stores/toast';
	import { ArrowLeft, Plus, AlertCircle } from 'lucide-svelte';

	const queryClient = useQueryClient();

	let templateKey = '';
	let name = '';
	let moduleKey = '';
	let description = '';
	let folderStructureJson = '[\n  "subfolder-1",\n  "subfolder-2"\n]';
	let defaultFilesJson = '[\n  {\n    "path": "README.md",\n    "content": "# Hello",\n    "contentType": "text/markdown"\n  }\n]';
	let metadataSchemaJson = '{}';
	let renderer = '';
	let visibilityPolicy = 'workspace';
	let error = '';

	const modulesQuery = createQuery({
		queryKey: ['admin-modules'],
		queryFn: () => listAdminModules()
	});

	const createMutation = createMutation({
		mutationFn: createTemplate,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			toast.success('Template created');
			goto('/admin/templates');
		},
		onError: (err: Error) => {
			toast.error(err.message);
		}
	});

	function validateJson(label: string, value: string): unknown {
		try {
			return JSON.parse(value);
		} catch (e) {
			throw new Error(`Invalid JSON in ${label}: ${e instanceof Error ? e.message : String(e)}`);
		}
	}

	function handleSubmit() {
		error = '';

		if (!templateKey.trim() || !name.trim() || !moduleKey.trim()) {
			error = 'Template key, name, and module are required.';
			return;
		}

		try {
			const folderStructure = validateJson('Folder Structure', folderStructureJson) as string[];
			const defaultFiles = validateJson('Default Files', defaultFilesJson);
			const metadataSchema = validateJson('Metadata Schema', metadataSchemaJson);

			$createMutation.mutate({
				template_key: templateKey.trim(),
				name: name.trim(),
				module_key: moduleKey,
				description: description.trim(),
				folder_structure: Array.isArray(folderStructure) ? folderStructure : [],
				default_files: Array.isArray(defaultFiles) ? defaultFiles : [],
				metadata_schema: metadataSchema as Record<string, unknown>,
				renderer: renderer.trim() || null,
				visibility_policy: visibilityPolicy
			});
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
	}
</script>

<svelte:head>
	<title>New Template - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-3xl">
	<a
		href="/admin/templates"
		class="mb-4 inline-flex items-center gap-1.5 text-sm text-base-content/50 transition-colors hover:text-base-content"
	>
		<ArrowLeft size={14} />
		Back to Templates
	</a>

	<h1 class="text-2xl font-semibold text-base-content">New Template</h1>
	<p class="mt-1 text-sm text-base-content/60">Create a custom template for workspace modules.</p>

	{#if error}
		<div class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error">
			<AlertCircle size={16} />
			{error}
		</div>
	{/if}

	<form on:submit|preventDefault={handleSubmit} class="mt-6 flex flex-col gap-4">
		<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="mb-4 text-sm font-semibold uppercase tracking-wider text-base-content">
				Basic Information
			</h2>
			<div class="grid gap-4">
				<div class="grid gap-4 sm:grid-cols-2">
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70">Template Key *</label>
						<input
							type="text"
							class="input input-bordered input-sm"
							placeholder="my-custom-template"
							bind:value={templateKey}
							required
						/>
						<p class="text-[10px] text-base-content/40">Unique identifier, e.g. template_custom_notes</p>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70">Template Name *</label>
						<input
							type="text"
							class="input input-bordered input-sm"
							placeholder="My Custom Template"
							bind:value={name}
							required
						/>
					</div>
				</div>

				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70">Module *</label>
					<select class="select select-bordered select-sm" bind:value={moduleKey} required>
						<option value="" disabled>Select a module</option>
						{#each $modulesQuery.data ?? [] as mod}
							<option value={mod.module_key}>{mod.display_name}</option>
						{/each}
					</select>
				</div>

				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70">Description</label>
					<textarea
						class="textarea textarea-bordered textarea-sm"
						placeholder="What this template creates..."
						bind:value={description}
						rows={2}
					></textarea>
				</div>

				<div class="grid gap-4 sm:grid-cols-2">
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70">Renderer</label>
						<input
							type="text"
							class="input input-bordered input-sm"
							placeholder="e.g. notes, kanban"
							bind:value={renderer}
						/>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70">Visibility Policy</label>
						<select class="select select-bordered select-sm" bind:value={visibilityPolicy}>
							<option value="workspace">Workspace</option>
							<option value="admin-only">Admin Only</option>
							<option value="public">Public</option>
						</select>
					</div>
				</div>
			</div>
		</div>

		<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="mb-4 text-sm font-semibold uppercase tracking-wider text-base-content">
				Structure & Content
			</h2>
			<div class="flex flex-col gap-4">
				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70">Folder Structure (JSON array)</label>
					<textarea
						class="font-mono textarea textarea-bordered textarea-sm text-xs"
						bind:value={folderStructureJson}
						rows={4}
					></textarea>
				</div>

				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70">Default Files (JSON array)</label>
					<textarea
						class="font-mono textarea textarea-bordered textarea-sm text-xs"
						bind:value={defaultFilesJson}
						rows={6}
					></textarea>
				</div>

				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70">Metadata Schema (JSON object)</label>
					<textarea
						class="font-mono textarea textarea-bordered textarea-sm text-xs"
						bind:value={metadataSchemaJson}
						rows={4}
					></textarea>
				</div>
			</div>
		</div>

		<div class="flex items-center justify-end gap-3">
			<a href="/admin/templates" class="btn btn-ghost btn-sm">Cancel</a>
			<button type="submit" class="btn btn-primary btn-sm" disabled={$createMutation.isPending}>
				{#if $createMutation.isPending}
					<span class="loading loading-spinner loading-xs"></span>
				{:else}
					<Plus size={14} />
				{/if}
				<span>Create Template</span>
			</button>
		</div>
	</form>
</div>
