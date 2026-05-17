<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import { getAdminTemplate, updateTemplate, listAdminModules } from '$lib/api/admin-modules';
	import { APPROVED_MODULE_ICONS } from '$lib/modules/iconRegistry';
	import { toastStore } from '$lib/stores/toast';
	import { ArrowLeft, Save, AlertCircle, Plus, Trash2 } from 'lucide-svelte';

	const queryClient = useQueryClient();
	const key = $page.params.key!;

	let name = $state('');
	let moduleKey = $state('');
	let description = $state('');
	let createLabel = $state('');
	let icon = $state('file-text');
	let folderStructureJson = $state('[]');
	let defaultFilesJson = $state('[]');
	let metadataSchemaJson = $state('{}');
	let renderer = $state('');
	let visibilityPolicy = $state('workspace');
	let enabled = $state(true);
	let isSystemTemplate = $state(false);
	let error = $state('');
	let moduleConfigJson = $state('{}');
	const templateQuery = createQuery({
		queryKey: ['admin-template', key],
		queryFn: () => getAdminTemplate(key)
	});

	const modulesQuery = createQuery({
		queryKey: ['admin-modules'],
		queryFn: () => listAdminModules()
	});

	$effect(() => {
		if ($templateQuery.data) {
			const t = $templateQuery.data;
			name = t.name;
			moduleKey = t.module_key;
			description = t.description ?? '';
			createLabel = t.ui_config?.createLabel ?? '';
			icon = t.ui_config?.icon ?? 'file-text';
			folderStructureJson = JSON.stringify(t.folder_structure ?? [], null, 2);
			defaultFilesJson = JSON.stringify(t.default_files ?? [], null, 2);
			metadataSchemaJson = JSON.stringify(t.metadata_schema ?? {}, null, 2);
			renderer = t.renderer ?? '';
			visibilityPolicy = t.visibility_policy ?? 'workspace';
			enabled = t.enabled ?? true;
			isSystemTemplate = t.system_template ?? false;
			moduleConfigJson = JSON.stringify(t.module_config ?? {}, null, 2);
		}
	});

	const updateMutation = createMutation({
		mutationFn: (payload: {
			name: string;
			module_key: string;
			description: string;
			ui_config: {
				createLabel?: string;
				icon?: string;
			};
			folder_structure: string[];
			default_files: { path: string; content?: string; content_type?: string }[];
			metadata_schema: Record<string, unknown>;
			renderer: string | null;
			visibility_policy: string;
			enabled: boolean;
			module_config: Record<string, unknown>;
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

	function getStandardKanbanColumns() {
		return [
			{ id: 'column_backlog', title: 'Backlog', slug: '00-Backlog', order: 0, status: 'backlog' },
			{ id: 'column_ready', title: 'Ready', slug: '01-Ready', order: 1, status: 'ready' },
			{ id: 'column_in_progress', title: 'In Progress', slug: '02-In-Progress', order: 2, status: 'in_progress' },
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
			config = JSON.parse(moduleConfigJson);
		} catch {}
		if (!config.kanban) config.kanban = {};
		if (!config.kanban.columns || !Array.isArray(config.kanban.columns) || config.kanban.columns.length === 0) {
			config.kanban.columns = getStandardKanbanColumns();
		}
		if (!config.kanban.labels || !Array.isArray(config.kanban.labels) || config.kanban.labels.length === 0) {
			config.kanban.labels = getDefaultKanbanLabels();
		}
		if (!config.kanban.settings) {
			config.kanban.settings = getDefaultKanbanSettings();
		}
		return config.kanban;
	}

	function syncKanbanConfig(kb: any) {
		moduleConfigJson = JSON.stringify({ kanban: kb }, null, 2);
		if (Array.isArray(kb.columns)) {
			folderStructureJson = JSON.stringify(
				kb.columns
					.slice()
					.sort((a: any, b: any) => Number(a.order ?? 0) - Number(b.order ?? 0))
					.map((column: any) => column.slug)
					.filter(Boolean),
				null,
				2
			);
		}
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

		if (!name.trim() || !moduleKey.trim()) {
			error = 'Template name and module are required.';
			return;
		}

		try {
			const folderStructure = validateJson('Folder Structure', folderStructureJson) as string[];
			const defaultFiles = validateJson('Default Files', defaultFilesJson);
			const metadataSchema = validateJson('Metadata Schema', metadataSchemaJson);
			const moduleConfig = validateJson('Module Config', moduleConfigJson);

			$updateMutation.mutate({
				name: name.trim(),
				module_key: moduleKey,
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
				enabled,
				module_config: moduleConfig as Record<string, unknown>
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
		{#if isSystemTemplate}
			<div
				class="mt-4 flex items-center gap-2 rounded-xl border border-warning/30 bg-warning/5 p-3 text-sm text-warning"
			>
				<AlertCircle size={16} />
				This is a system template. Structure, files, and schema cannot be modified directly. Duplicate
				this template to create a customizable version.
			</div>
		{/if}

		{#if error}
			<div
				class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error"
			>
				<AlertCircle size={16} />
				{error}
			</div>
		{/if}

		<form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="mt-6 flex flex-col gap-4">
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

					<div class="grid gap-4 sm:grid-cols-4">
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
							disabled={isSystemTemplate}
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
							disabled={isSystemTemplate}
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
							disabled={isSystemTemplate}
						></textarea>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Module Configuration
				</h2>
				{#if moduleKey === 'kanban'}
					<div class="flex flex-col gap-5">
						<!-- Columns -->
						<div>
							<div class="mb-2 flex items-center justify-between">
								<span class="text-xs font-semibold text-base-content/70">Columns</span>
								<button
									type="button"
									class="btn btn-ghost btn-xs gap-1"
									onclick={addKanbanColumn}
									disabled={isSystemTemplate}
								>
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
											disabled={isSystemTemplate}
										/>
										<input
											class="input-bordered input input-sm w-28"
											value={column.slug}
											oninput={(e) => updateKanbanColumn(i, 'slug', e.currentTarget.value)}
											placeholder="Slug"
											disabled={isSystemTemplate}
										/>
										<input
											class="input-bordered input input-sm w-16"
											type="number"
											value={column.order}
											oninput={(e) => updateKanbanColumn(i, 'order', parseInt(e.currentTarget.value) || 0)}
											placeholder="Order"
											disabled={isSystemTemplate}
										/>
										<select
											class="select-bordered select select-sm w-28"
											value={column.status}
											onchange={(e) => updateKanbanColumn(i, 'status', e.currentTarget.value)}
											disabled={isSystemTemplate}
										>
											<option value="backlog">Backlog</option>
											<option value="ready">Ready</option>
											<option value="in_progress">In Progress</option>
											<option value="review">Review</option>
											<option value="done">Done</option>
										</select>
										{#if !isSystemTemplate}
											<button
												type="button"
												class="btn btn-ghost btn-xs text-error"
												onclick={() => removeKanbanColumn(i)}
											>
												<Trash2 size={12} />
											</button>
										{/if}
									</div>
								{/each}
							</div>
						</div>

						<!-- Labels -->
						<div>
							<div class="mb-2 flex items-center justify-between">
								<span class="text-xs font-semibold text-base-content/70">Labels</span>
								<button
									type="button"
									class="btn btn-ghost btn-xs gap-1"
									onclick={addKanbanLabel}
									disabled={isSystemTemplate}
								>
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
											disabled={isSystemTemplate}
										/>
										<select
											class="select-bordered select select-sm w-24"
											value={label.color}
											onchange={(e) => updateKanbanLabel(i, 'color', e.currentTarget.value)}
											disabled={isSystemTemplate}
										>
											<option value="green">Green</option>
											<option value="yellow">Yellow</option>
											<option value="orange">Orange</option>
											<option value="red">Red</option>
											<option value="purple">Purple</option>
											<option value="blue">Blue</option>
											<option value="gray">Gray</option>
										</select>
										{#if !isSystemTemplate}
											<button
												type="button"
												class="btn btn-ghost btn-xs text-error"
												onclick={() => removeKanbanLabel(i)}
											>
												<Trash2 size={12} />
											</button>
										{/if}
									</div>
								{/each}
							</div>
						</div>

						<!-- Settings -->
						<div>
							<span class="text-xs font-semibold text-base-content/70">Settings</span>
							<div class="mt-2 grid gap-2 sm:grid-cols-2">
								{#each Object.entries(ensureKanbanConfig().settings) as [key, value]}
									<div class="flex items-center gap-2">
										<input
											id={`kanban-setting-${key}`}
											type="checkbox"
											class="checkbox checkbox-sm"
											checked={value as boolean}
											onchange={(e) => updateKanbanSetting(key, e.currentTarget.checked)}
											disabled={isSystemTemplate}
										/>
										<label for={`kanban-setting-${key}`} class="text-xs text-base-content/80 capitalize">{key.replace(/_/g, ' ')}</label>
									</div>
								{/each}
							</div>
						</div>
					</div>
				{:else}
					<div class="flex flex-col gap-1">
						<label
							class="text-xs font-semibold text-base-content/70"
							for="module-config-json-object">Module Config (JSON object)</label
						>
						<textarea
							id="module-config-json-object"
							class="textarea-bordered textarea font-mono text-xs textarea-sm"
							bind:value={moduleConfigJson}
							rows={4}
							disabled={isSystemTemplate}
						></textarea>
					</div>
				{/if}
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
