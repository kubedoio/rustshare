<script lang="ts">
	import { goto } from '$app/navigation';
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import { createTemplate } from '$lib/api/admin-applications';
	import { listAdminModules } from '$lib/api/admin-applications';
	import { APPROVED_MODULE_ICONS } from '$lib/applications/iconRegistry';
	import { toastStore } from '$lib/stores/toast';
	import { ArrowLeft, Plus, AlertCircle, Trash2 } from 'lucide-svelte';

	const queryClient = useQueryClient();

	let templateKey = '';
	let name = '';
	let applicationId = '';
	let description = '';
	let createLabel = '';
	let icon = 'file-text';
	let folderStructureJson = '[\n  "subfolder-1",\n  "subfolder-2"\n]';
	let defaultFilesJson =
		'[\n  {\n    "path": "README.md",\n    "content": "# Hello",\n    "contentType": "text/markdown"\n  }\n]';
	let metadataSchemaJson = '{}';
	let renderer = '';
	let visibilityPolicy = 'workspace';
	let error = '';
	let applicationConfigJson = '{}';
	const modulesQuery = createQuery({
		queryKey: ['admin-applications'],
		queryFn: () => listAdminModules()
	});

	const createTemplateMutation = createMutation({
		mutationFn: createTemplate,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin-templates'] });
			toastStore.show('Template created', 'success');
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

	function getStandardKanbanColumns() {
		return [
			{ id: 'column_backlog', title: 'Backlog', slug: '00-Backlog', order: 0, status: 'backlog' },
			{ id: 'column_ready', title: 'Ready', slug: '01-Ready', order: 1, status: 'ready' },
			{
				id: 'column_in_progress',
				title: 'In Progress',
				slug: '02-In-Progress',
				order: 2,
				status: 'in_progress'
			},
			{ id: 'column_review', title: 'Review', slug: '03-Review', order: 3, status: 'review' },
			{ id: 'column_done', title: 'Done', slug: '04-Done', order: 4, status: 'done' }
		];
	}

	function getDefaultKanbanLabels() {
		return [
			{ id: 'label_green', name: 'Low', color: 'green' },
			{ id: 'label_yellow', name: 'Medium', color: 'yellow' },
			{ id: 'label_orange', name: 'High', color: 'orange' },
			{ id: 'label_red', name: 'Urgent', color: 'red' }
		];
	}

	function getDefaultKanbanSettings() {
		return {
			show_description_on_cards: true,
			description_preview_lines: 2,
			show_assignees: true,
			show_labels: true,
			show_due_date: true,
			show_attachment_badge: true,
			show_checklist_badge: true
		};
	}

	function ensureKanbanConfig() {
		let config: any = {};
		try {
			config = JSON.parse(applicationConfigJson);
		} catch {
			/* ignore parse error */
		}
		if (!config.kanban) config.kanban = {};
		if (
			!config.kanban.columns ||
			!Array.isArray(config.kanban.columns) ||
			config.kanban.columns.length === 0
		) {
			config.kanban.columns = getStandardKanbanColumns();
		}
		if (
			!config.kanban.labels ||
			!Array.isArray(config.kanban.labels) ||
			config.kanban.labels.length === 0
		) {
			config.kanban.labels = getDefaultKanbanLabels();
		}
		if (!config.kanban.settings) {
			config.kanban.settings = getDefaultKanbanSettings();
		}
		return config.kanban;
	}

	function syncKanbanConfig(kb: any) {
		applicationConfigJson = JSON.stringify({ kanban: kb }, null, 2);
	}

	function addKanbanColumn() {
		const kb = ensureKanbanConfig();
		const order = kb.columns.length;
		kb.columns.push({
			id: `column_${Date.now()}`,
			title: 'New Column',
			slug: `${String(order).padStart(2, '0')}-new-column`,
			order,
			status: 'backlog'
		});
		syncKanbanConfig(kb);
	}

	function removeKanbanColumn(index: number) {
		const kb = ensureKanbanConfig();
		kb.columns.splice(index, 1);
		kb.columns.forEach((c: any, i: number) => {
			c.order = i;
		});
		syncKanbanConfig(kb);
	}

	function updateKanbanColumn(index: number, field: string, value: any) {
		const kb = ensureKanbanConfig();
		kb.columns[index][field] = value;
		syncKanbanConfig(kb);
	}

	function addKanbanLabel() {
		const kb = ensureKanbanConfig();
		kb.labels.push({
			id: `label_${Date.now()}`,
			name: 'New Label',
			color: 'gray'
		});
		syncKanbanConfig(kb);
	}

	function removeKanbanLabel(index: number) {
		const kb = ensureKanbanConfig();
		kb.labels.splice(index, 1);
		syncKanbanConfig(kb);
	}

	function updateKanbanLabel(index: number, field: string, value: any) {
		const kb = ensureKanbanConfig();
		kb.labels[index][field] = value;
		syncKanbanConfig(kb);
	}

	function updateKanbanSetting(key: string, value: boolean) {
		const kb = ensureKanbanConfig();
		kb.settings[key] = value;
		syncKanbanConfig(kb);
	}

	function handleSubmit() {
		error = '';

		if (!templateKey.trim() || !name.trim() || !applicationId.trim()) {
			error = 'Template key, name, and module are required.';
			return;
		}

		try {
			const folderStructure = validateJson('Folder Structure', folderStructureJson) as string[];
			const defaultFiles = validateJson('Default Files', defaultFilesJson);
			const metadataSchema = validateJson('Metadata Schema', metadataSchemaJson);
			const applicationConfig = validateJson('Application Config', applicationConfigJson);

			$createTemplateMutation.mutate({
				template_key: templateKey.trim(),
				name: name.trim(),
				application_id: applicationId,
				description: description.trim(),
				ui_config: {
					createLabel: createLabel.trim() || undefined,
					icon: icon.trim() || undefined
				},
				folder_structure: Array.isArray(folderStructure) ? folderStructure : [],
				default_files: Array.isArray(defaultFiles) ? defaultFiles : [],
				metadata_schema: metadataSchema as Record<string, unknown>,
				renderer: renderer.trim() || null,
				visibility_policy: visibilityPolicy,
				application_config: applicationConfig as Record<string, unknown>
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
		<div
			class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error"
		>
			<AlertCircle size={16} />
			{error}
		</div>
	{/if}

	<form
		onsubmit={(e) => {
			e.preventDefault();
			handleSubmit();
		}}
		class="mt-6 flex flex-col gap-4"
	>
		<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
				Basic Information
			</h2>
			<div class="grid gap-4">
				<div class="grid gap-4 sm:grid-cols-2">
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="template-key"
							>Template Key *</label
						>
						<input
							id="template-key"
							type="text"
							class="input-bordered input input-sm"
							placeholder="my-custom-template"
							bind:value={templateKey}
							required
						/>
						<p class="text-[10px] text-base-content/40">
							Unique identifier, e.g. template_custom_notes
						</p>
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
					<label class="text-xs font-semibold text-base-content/70" for="module"
						>Application *</label
					>
					<select
						id="module"
						class="select-bordered select select-sm"
						bind:value={applicationId}
						required
					>
						<option value="" disabled>Select a module</option>
						{#each $modulesQuery.data ?? [] as mod}
							<option value={mod.application_id}>{mod.display_name}</option>
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
						rows={2}></textarea>
				</div>

				<div class="grid gap-4 sm:grid-cols-3">
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="create-label"
							>Create Label</label
						>
						<input
							id="create-label"
							type="text"
							class="input-bordered input input-sm"
							placeholder="New Note"
							bind:value={createLabel}
						/>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="icon">Icon</label>
						<select id="icon" class="select-bordered select select-sm" bind:value={icon}>
							{#each APPROVED_MODULE_ICONS as approvedIcon}
								<option value={approvedIcon}>{approvedIcon}</option>
							{/each}
						</select>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="renderer">Renderer</label
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
						rows={4}></textarea>
				</div>

				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70" for="default-files-json-array"
						>Default Files (JSON array)</label
					>
					<textarea
						id="default-files-json-array"
						class="textarea-bordered textarea font-mono text-xs textarea-sm"
						bind:value={defaultFilesJson}
						rows={6}></textarea>
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
						rows={4}></textarea>
				</div>
			</div>
		</div>

		<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
			<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
				Application Configuration
			</h2>
			{#if applicationId === 'kanban'}
				<div class="flex flex-col gap-5">
					<!-- Columns -->
					<div>
						<div class="mb-2 flex items-center justify-between">
							<span class="text-xs font-semibold text-base-content/70">Columns</span>
							<button type="button" class="btn btn-ghost btn-xs gap-1" onclick={addKanbanColumn}>
								<Plus size={12} />
								<span>Add Column</span>
							</button>
						</div>
						<div class="flex flex-col gap-2">
							{#each ensureKanbanConfig().columns as column, i}
								<div class="flex items-center gap-2">
									<input
										class="input-bordered input input-sm w-28"
										value={column.title}
										oninput={(e) => updateKanbanColumn(i, 'title', e.currentTarget.value)}
										placeholder="Title"
									/>
									<input
										class="input-bordered input input-sm w-28"
										value={column.slug}
										oninput={(e) => updateKanbanColumn(i, 'slug', e.currentTarget.value)}
										placeholder="Slug"
									/>
									<input
										class="input-bordered input input-sm w-16"
										type="number"
										value={column.order}
										oninput={(e) =>
											updateKanbanColumn(i, 'order', parseInt(e.currentTarget.value) || 0)}
										placeholder="Order"
									/>
									<select
										class="select-bordered select select-sm w-28"
										value={column.status}
										onchange={(e) => updateKanbanColumn(i, 'status', e.currentTarget.value)}
									>
										<option value="backlog">Backlog</option>
										<option value="ready">Ready</option>
										<option value="in_progress">In Progress</option>
										<option value="review">Review</option>
										<option value="done">Done</option>
									</select>
									<button
										type="button"
										class="btn btn-ghost btn-xs text-error"
										onclick={() => removeKanbanColumn(i)}
									>
										<Trash2 size={12} />
									</button>
								</div>
							{/each}
						</div>
					</div>

					<!-- Labels -->
					<div>
						<div class="mb-2 flex items-center justify-between">
							<span class="text-xs font-semibold text-base-content/70">Labels</span>
							<button type="button" class="btn btn-ghost btn-xs gap-1" onclick={addKanbanLabel}>
								<Plus size={12} />
								<span>Add Label</span>
							</button>
						</div>
						<div class="flex flex-col gap-2">
							{#each ensureKanbanConfig().labels as label, i}
								<div class="flex items-center gap-2">
									<input
										class="input-bordered input input-sm w-32"
										value={label.name}
										oninput={(e) => updateKanbanLabel(i, 'name', e.currentTarget.value)}
										placeholder="Name"
									/>
									<select
										class="select-bordered select select-sm w-24"
										value={label.color}
										onchange={(e) => updateKanbanLabel(i, 'color', e.currentTarget.value)}
									>
										<option value="green">Green</option>
										<option value="yellow">Yellow</option>
										<option value="orange">Orange</option>
										<option value="red">Red</option>
										<option value="purple">Purple</option>
										<option value="blue">Blue</option>
										<option value="gray">Gray</option>
									</select>
									<button
										type="button"
										class="btn btn-ghost btn-xs text-error"
										onclick={() => removeKanbanLabel(i)}
									>
										<Trash2 size={12} />
									</button>
								</div>
							{/each}
						</div>
					</div>

					<!-- Settings -->
					<div>
						<span class="text-xs font-semibold text-base-content/70">Settings</span>
						<div class="mt-2 grid gap-2 sm:grid-cols-2">
							{#each Object.entries(ensureKanbanConfig().settings) as [key, value]}
								<label class="flex items-center gap-2">
									<input
										type="checkbox"
										class="checkbox checkbox-sm"
										checked={value as boolean}
										onchange={(e) => updateKanbanSetting(key, e.currentTarget.checked)}
									/>
									<span class="text-xs text-base-content/80 capitalize"
										>{key.replace(/_/g, ' ')}</span
									>
								</label>
							{/each}
						</div>
					</div>
				</div>
			{:else}
				<div class="flex flex-col gap-1">
					<label class="text-xs font-semibold text-base-content/70" for="module-config-json-object"
						>Application Config (JSON object)</label
					>
					<textarea
						id="module-config-json-object"
						class="textarea-bordered textarea font-mono text-xs textarea-sm"
						bind:value={applicationConfigJson}
						rows={4}></textarea>
				</div>
			{/if}
		</div>

		<div class="flex items-center justify-end gap-3">
			<a href="/admin/templates" class="btn btn-ghost btn-sm">Cancel</a>
			<button
				type="submit"
				class="btn btn-sm btn-primary"
				disabled={$createTemplateMutation.isPending}
			>
				{#if $createTemplateMutation.isPending}
					<span class="loading loading-xs loading-spinner"></span>
				{:else}
					<Plus size={14} />
				{/if}
				<span>Create Template</span>
			</button>
		</div>
	</form>
</div>
