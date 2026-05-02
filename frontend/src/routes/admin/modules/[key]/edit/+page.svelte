<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import { getAdminModule, updateModule, listAdminTemplates } from '$lib/api/admin-modules';
	import { APPROVED_MODULE_ICONS } from '$lib/modules/iconRegistry';
	import { toastStore } from '$lib/stores/toast';
	import { refreshModules } from '$lib/modules/registry';
	import { ArrowLeft, Save, AlertCircle } from 'lucide-svelte';

	const queryClient = useQueryClient();
	const key = $page.params.key!;

	let displayName = '';
	let description = '';
	let icon = 'file-text';
	let rootPath = '';
	let renderer = '';
	let defaultTemplate = '';
	let enabled = false;

	let sidebarEnabled = true;
	let sidebarOrder = 30;
	let sidebarIcon = 'file-text';
	let sidebarLabel = '';

	let dashboardEnabled = true;
	let dashboardOrder = 10;
	let dashboardCardTitle = '';
	let dashboardCardDescription = '';
	let dashboardSummaryMode = 'recent-items';
	let dashboardWidgetEnabled = true;
	let dashboardWidgetType = 'generic-module-summary';
	let dashboardWidgetSize: 'small' | 'medium' | 'large' = 'small';
	let dashboardColumnsDesktop = 3;
	let dashboardColumnsTablet = 6;
	let dashboardColumnsMobile = 12;
	let dashboardMaxItems = 4;
	let primaryActionLabel = '';
	let primaryActionAction = 'create-from-template';
	let primaryActionTemplate = '';
	let pageEnabled = true;
	let pageRoute = '';
	let pageRenderer = '';
	let modulePageLayout = 'list-grid';
	let modulePageEmptyStateTitle = '';
	let modulePageEmptyStateDescription = '';
	let modulePageEmptyStateAction = '';

	let error = '';

	const moduleQuery = createQuery({
		queryKey: ['admin-module', key],
		queryFn: () => getAdminModule(key)
	});

	const templatesQuery = createQuery({
		queryKey: ['admin-templates'],
		queryFn: () => listAdminTemplates()
	});

	$: moduleTemplates = ($templatesQuery.data ?? []).filter((t) => t.module_key === key);

	$: if ($moduleQuery.data) {
		const m = $moduleQuery.data;
		displayName = m.display_name;
		description = m.description ?? '';
		icon = m.icon ?? 'file-text';
		rootPath = m.root_path ?? '';
		renderer = m.renderer ?? '';
		defaultTemplate = m.default_template ?? '';
		enabled = m.enabled ?? false;

		const ui = m.ui_config ?? {};
		sidebarEnabled = ui.sidebar?.enabled ?? true;
		sidebarOrder = ui.sidebar?.order ?? 30;
		sidebarIcon = ui.sidebar?.icon ?? icon;
		sidebarLabel = ui.sidebar?.label ?? displayName;

		dashboardEnabled = ui.dashboard?.enabled ?? true;
		dashboardOrder = ui.dashboard?.order ?? 10;
		dashboardCardTitle = ui.dashboard?.cardTitle ?? displayName;
		dashboardCardDescription = ui.dashboard?.cardDescription ?? description;
		dashboardSummaryMode = ui.dashboard?.summaryMode ?? 'recent-items';
		dashboardWidgetEnabled = ui.dashboard?.widget?.enabled ?? dashboardEnabled;
		dashboardWidgetType = ui.dashboard?.widget?.type ?? dashboardSummaryMode;
		dashboardWidgetSize = ui.dashboard?.widget?.size ?? 'small';
		dashboardColumnsDesktop = ui.dashboard?.widget?.columns?.desktop ?? 3;
		dashboardColumnsTablet = ui.dashboard?.widget?.columns?.tablet ?? 6;
		dashboardColumnsMobile = ui.dashboard?.widget?.columns?.mobile ?? 12;
		dashboardMaxItems = ui.dashboard?.maxItems ?? 4;
		primaryActionLabel = ui.dashboard?.primaryAction?.label ?? '';
		primaryActionAction = ui.dashboard?.primaryAction?.action ?? 'create-from-template';
		primaryActionTemplate = ui.dashboard?.primaryAction?.template ?? m.default_template ?? '';
		pageEnabled = ui.page?.enabled ?? true;
		pageRoute = ui.page?.route ?? `/modules/${key}`;
		pageRenderer = ui.page?.renderer ?? renderer ?? key;
		modulePageLayout = ui.page?.layout ?? ui.modulePage?.layout ?? 'list-grid';
		modulePageEmptyStateTitle = ui.page?.emptyStateTitle ?? ui.modulePage?.emptyStateTitle ?? '';
		modulePageEmptyStateDescription =
			ui.page?.emptyStateDescription ?? ui.modulePage?.emptyStateDescription ?? '';
		modulePageEmptyStateAction = ui.page?.emptyStateAction ?? ui.modulePage?.emptyStateAction ?? '';
	}

	const updateMutation = createMutation({
		mutationFn: (payload: {
			display_name: string;
			description: string;
			icon: string;
			root_path: string;
			renderer: string | null;
			default_template: string | null;
			ui_config: {
				sidebar: { enabled: boolean; order: number; icon: string; label: string };
				dashboard: {
					enabled: boolean;
					order: number;
					cardTitle: string;
					cardDescription: string;
					summaryMode: string;
					maxItems: number;
					widget: {
						enabled: boolean;
						type: string;
						title: string;
						description: string;
						size: 'small' | 'medium' | 'large';
						columns: { desktop: number; tablet: number; mobile: number };
						maxItems: number;
						primaryAction: {
							label: string;
							action: string;
							template?: string;
						};
					};
					primaryAction: {
						label: string;
						action: string;
						template?: string;
					};
				};
				page: {
					enabled: boolean;
					route: string;
					renderer: string;
					layout: string;
					emptyStateTitle: string;
					emptyStateDescription: string;
					emptyStateAction: string;
					primaryAction: {
						label: string;
						action: string;
						template?: string;
					};
				};
				modulePage: {
					layout: string;
					emptyStateTitle: string;
					emptyStateDescription: string;
					emptyStateAction: string;
				};
			};
		}) => updateModule(key, payload),
		onSuccess: () => {
			refreshModules();
			queryClient.invalidateQueries({ queryKey: ['admin-modules'] });
			queryClient.invalidateQueries({ queryKey: ['admin-module', key] });
			queryClient.invalidateQueries({ queryKey: ['enabled-modules'] });
			toastStore.show('Module updated', 'success');
			goto('/admin/modules');
		},
		onError: (err: Error) => {
			toastStore.show(err.message, 'error');
		}
	});

	function handleSubmit() {
		error = '';

		if (!displayName.trim()) {
			error = 'Display name is required.';
			return;
		}

		$updateMutation.mutate({
			display_name: displayName.trim(),
			description: description.trim(),
			icon: icon.trim(),
			root_path: rootPath.trim(),
			renderer: renderer.trim() || null,
			default_template: defaultTemplate.trim() || null,
			ui_config: {
				sidebar: {
					enabled: sidebarEnabled,
					order: sidebarOrder,
					icon: sidebarIcon.trim(),
					label: sidebarLabel.trim() || displayName.trim()
				},
				dashboard: {
					enabled: dashboardEnabled,
					order: dashboardOrder,
					cardTitle: dashboardCardTitle.trim() || displayName.trim(),
					cardDescription: dashboardCardDescription.trim() || description.trim(),
					summaryMode: dashboardWidgetType || dashboardSummaryMode,
					maxItems: dashboardMaxItems,
					widget: {
						enabled: dashboardWidgetEnabled,
						type: dashboardWidgetType.trim() || dashboardSummaryMode,
						title: dashboardCardTitle.trim() || displayName.trim(),
						description: dashboardCardDescription.trim() || description.trim(),
						size: dashboardWidgetSize as 'small' | 'medium' | 'large',
						columns: {
							desktop: Number(dashboardColumnsDesktop) || 3,
							tablet: Number(dashboardColumnsTablet) || 6,
							mobile: Number(dashboardColumnsMobile) || 12
						},
						maxItems: Number(dashboardMaxItems) || 4,
						primaryAction: {
							label: primaryActionLabel.trim() || modulePageEmptyStateAction.trim() || 'Open',
							action: primaryActionAction.trim() || 'create-from-template',
							...(primaryActionTemplate.trim() ? { template: primaryActionTemplate.trim() } : {})
						}
					},
					primaryAction: {
						label: primaryActionLabel.trim() || modulePageEmptyStateAction.trim() || 'Open',
						action: primaryActionAction.trim() || 'create-from-template',
						...(primaryActionTemplate.trim() ? { template: primaryActionTemplate.trim() } : {})
					}
				},
				page: {
					enabled: pageEnabled,
					route: pageRoute.trim() || `/modules/${key}`,
					renderer: pageRenderer.trim() || renderer.trim() || key,
					layout: modulePageLayout.trim() || 'list-grid',
					emptyStateTitle:
						modulePageEmptyStateTitle.trim() || `No ${displayName.trim().toLowerCase()} yet`,
					emptyStateDescription: modulePageEmptyStateDescription.trim() || description.trim(),
					emptyStateAction:
						modulePageEmptyStateAction.trim() || primaryActionLabel.trim() || 'Create',
					primaryAction: {
						label: primaryActionLabel.trim() || modulePageEmptyStateAction.trim() || 'Open',
						action: primaryActionAction.trim() || 'create-from-template',
						...(primaryActionTemplate.trim() ? { template: primaryActionTemplate.trim() } : {})
					}
				},
				modulePage: {
					layout: modulePageLayout.trim() || 'list-grid',
					emptyStateTitle:
						modulePageEmptyStateTitle.trim() || `No ${displayName.trim().toLowerCase()} yet`,
					emptyStateDescription: modulePageEmptyStateDescription.trim() || description.trim(),
					emptyStateAction:
						modulePageEmptyStateAction.trim() || primaryActionLabel.trim() || 'Create'
				}
			}
		});
	}
</script>

<svelte:head>
	<title>Edit Module - Admin - RustShare</title>
</svelte:head>

<div class="mx-auto max-w-3xl">
	<a
		href="/admin/modules"
		class="mb-4 inline-flex items-center gap-1.5 text-sm text-base-content/50 transition-colors hover:text-base-content"
	>
		<ArrowLeft size={14} />
		Back to Modules
	</a>

	<h1 class="text-2xl font-semibold text-base-content">Edit Module</h1>
	<p class="mt-1 text-sm text-base-content/60">
		Configure module visibility, appearance, and behavior.
	</p>

	{#if $moduleQuery.isLoading}
		<div class="flex h-64 items-center justify-center">
			<div class="loading loading-lg loading-spinner text-brand-500"></div>
		</div>
	{:else if $moduleQuery.isError}
		<div
			class="mt-4 flex items-center gap-2 rounded-xl border border-error/30 bg-error/5 p-3 text-sm text-error"
		>
			<AlertCircle size={16} />
			Failed to load module: {$moduleQuery.error?.message ?? 'Unknown error'}
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
							<label class="text-xs font-semibold text-base-content/70" for="module-key"
								>Module Key</label
							>
							<input
								id="module-key"
								type="text"
								class="input-bordered input input-sm bg-base-200/50"
								value={key}
								disabled
							/>
							<p class="text-[10px] text-base-content/40">Module key cannot be changed.</p>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="display-name"
								>Display Name *</label
							>
							<input
								id="display-name"
								type="text"
								class="input-bordered input input-sm"
								placeholder="Notes"
								bind:value={displayName}
								required
							/>
						</div>
					</div>

					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="description"
							>Description</label
						>
						<textarea
							id="description"
							class="textarea-bordered textarea textarea-sm"
							placeholder="What this module does..."
							bind:value={description}
							rows={2}
						></textarea>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="icon">Icon</label>
							<select id="icon" class="select-bordered select select-sm" bind:value={icon}>
								{#each APPROVED_MODULE_ICONS as ic}
									<option value={ic}>{ic}</option>
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
								bind:value={renderer}
							/>
							<p class="text-[10px] text-base-content/40">
								Renderer resolves the module page view.
							</p>
						</div>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="root-path"
								>Root Path</label
							>
							<input
								id="root-path"
								type="text"
								class="input-bordered input input-sm"
								bind:value={rootPath}
							/>
							<p class="text-[10px] text-base-content/40">
								Use a single absolute workspace path like `/Notes`.
							</p>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="default-template"
								>Default Template</label
							>
							<select
								id="default-template"
								class="select-bordered select select-sm"
								bind:value={defaultTemplate}
							>
								<option value="">None</option>
								{#each moduleTemplates as t}
									<option value={t.template_key}>{t.name}</option>
								{/each}
							</select>
						</div>
					</div>

					<div class="flex items-center gap-3">
						<input
							type="checkbox"
							class="checkbox checkbox-sm"
							bind:checked={enabled}
							id="enabled"
							disabled
						/>
						<label for="enabled" class="text-sm text-base-content/80">
							Enabled (use the toggle on the modules list to change)
						</label>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Sidebar
				</h2>
				<div class="grid gap-4">
					<div class="flex items-center gap-3">
						<input
							type="checkbox"
							class="checkbox checkbox-sm"
							bind:checked={sidebarEnabled}
							id="sidebar-enabled"
						/>
						<label for="sidebar-enabled" class="text-sm text-base-content/80">Show in sidebar</label
						>
					</div>

					<div class="grid gap-4 sm:grid-cols-3">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="order">Order</label>
							<input
								id="order"
								type="number"
								class="input-bordered input input-sm"
								bind:value={sidebarOrder}
							/>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="icon-1">Icon</label>
							<select id="icon-1" class="select-bordered select select-sm" bind:value={sidebarIcon}>
								{#each APPROVED_MODULE_ICONS as ic}
									<option value={ic}>{ic}</option>
								{/each}
							</select>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="label">Label</label>
							<input
								id="label"
								type="text"
								class="input-bordered input input-sm"
								placeholder="Notes"
								bind:value={sidebarLabel}
							/>
						</div>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Dashboard
				</h2>
				<div class="grid gap-4">
					<div class="flex items-center gap-3">
						<input
							type="checkbox"
							class="checkbox checkbox-sm"
							bind:checked={dashboardEnabled}
							id="dashboard-enabled"
						/>
						<label for="dashboard-enabled" class="text-sm text-base-content/80"
							>Show on dashboard</label
						>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="order-1">Order</label>
							<input
								id="order-1"
								type="number"
								class="input-bordered input input-sm"
								bind:value={dashboardOrder}
							/>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="card-title"
								>Card Title</label
							>
							<input
								id="card-title"
								type="text"
								class="input-bordered input input-sm"
								placeholder="Notes"
								bind:value={dashboardCardTitle}
							/>
						</div>
					</div>

					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="card-description"
							>Card Description</label
						>
						<textarea
							id="card-description"
							class="textarea-bordered textarea textarea-sm"
							placeholder="Short description shown on the dashboard card..."
							bind:value={dashboardCardDescription}
							rows={2}
						></textarea>
					</div>

					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="summary-mode"
								>Widget Type</label
							>
							<select
								id="summary-mode"
								class="select-bordered select select-sm"
								bind:value={dashboardWidgetType}
							>
								<option value="generic-module-summary">Generic Module Summary</option>
								<option value="kanban-summary">Kanban Summary</option>
								<option value="decisions-meetings-summary">Decisions & Meetings Summary</option>
								<option value="latest-notes">Latest Notes</option>
								<option value="active-shares">Active Shares</option>
							</select>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="max-items"
								>Max Items</label
							>
							<input
								id="max-items"
								type="number"
								min="1"
								max="10"
								class="input-bordered input input-sm"
								bind:value={dashboardMaxItems}
							/>
						</div>
					</div>

					<div class="grid gap-4 sm:grid-cols-4">
						<div class="flex items-center gap-3 sm:col-span-1">
							<input
								type="checkbox"
								class="checkbox checkbox-sm"
								bind:checked={dashboardWidgetEnabled}
								id="widget-enabled"
							/>
							<label for="widget-enabled" class="text-sm text-base-content/80">Widget enabled</label
							>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="widget-size"
								>Size</label
							>
							<select
								id="widget-size"
								class="select-bordered select select-sm"
								bind:value={dashboardWidgetSize}
							>
								<option value="small">Small</option>
								<option value="medium">Medium</option>
								<option value="large">Large</option>
							</select>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="widget-desktop"
								>Desktop Columns</label
							>
							<input
								id="widget-desktop"
								type="number"
								min="1"
								max="12"
								class="input-bordered input input-sm"
								bind:value={dashboardColumnsDesktop}
							/>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="widget-tablet"
								>Tablet Columns</label
							>
							<input
								id="widget-tablet"
								type="number"
								min="1"
								max="12"
								class="input-bordered input input-sm"
								bind:value={dashboardColumnsTablet}
							/>
						</div>
					</div>
					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="widget-mobile"
								>Mobile Columns</label
							>
							<input
								id="widget-mobile"
								type="number"
								min="1"
								max="12"
								class="input-bordered input input-sm"
								bind:value={dashboardColumnsMobile}
							/>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="summary-mode-legacy"
								>Legacy Summary Mode</label
							>
							<input
								id="summary-mode-legacy"
								type="text"
								class="input-bordered input input-sm"
								bind:value={dashboardSummaryMode}
							/>
						</div>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Primary Action
				</h2>
				<div class="grid gap-4 sm:grid-cols-3">
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="primary-action-label"
							>Label</label
						>
						<input
							id="primary-action-label"
							type="text"
							class="input-bordered input input-sm"
							bind:value={primaryActionLabel}
						/>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="primary-action-action"
							>Action</label
						>
						<input
							id="primary-action-action"
							type="text"
							class="input-bordered input input-sm"
							bind:value={primaryActionAction}
						/>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="primary-action-template"
							>Template</label
						>
						<select
							id="primary-action-template"
							class="select-bordered select select-sm"
							bind:value={primaryActionTemplate}
						>
							<option value="">None</option>
							{#each moduleTemplates as t}
								<option value={t.template_key}>{t.name}</option>
							{/each}
						</select>
					</div>
				</div>
			</div>

			<div class="rounded-2xl border border-base-300/50 bg-base-100 p-6 shadow-sm">
				<h2 class="mb-4 text-sm font-semibold tracking-wider text-base-content uppercase">
					Module Page
				</h2>
				<div class="grid gap-4">
					<div class="grid gap-4 sm:grid-cols-3">
						<div class="flex items-center gap-3">
							<input
								type="checkbox"
								class="checkbox checkbox-sm"
								bind:checked={pageEnabled}
								id="page-enabled"
							/>
							<label for="page-enabled" class="text-sm text-base-content/80">Page enabled</label>
						</div>
						<div class="flex flex-col gap-1 sm:col-span-1">
							<label class="text-xs font-semibold text-base-content/70" for="page-route"
								>Route</label
							>
							<input
								id="page-route"
								type="text"
								class="input-bordered input input-sm"
								bind:value={pageRoute}
							/>
						</div>
						<div class="flex flex-col gap-1 sm:col-span-1">
							<label class="text-xs font-semibold text-base-content/70" for="page-renderer"
								>Page Renderer</label
							>
							<input
								id="page-renderer"
								type="text"
								class="input-bordered input input-sm"
								bind:value={pageRenderer}
							/>
						</div>
					</div>
					<div class="grid gap-4 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="module-layout"
								>Layout</label
							>
							<select
								id="module-layout"
								class="select-bordered select select-sm"
								bind:value={modulePageLayout}
							>
								<option value="list-grid">List Grid</option>
								<option value="file-list">File List</option>
								<option value="gallery-grid">Gallery Grid</option>
								<option value="kanban-board">Kanban Board</option>
								<option value="decision-registry">Decision Registry</option>
								<option value="share-manager">Share Manager</option>
							</select>
						</div>
						<div class="flex flex-col gap-1">
							<label class="text-xs font-semibold text-base-content/70" for="empty-state-action"
								>Empty State Action</label
							>
							<input
								id="empty-state-action"
								type="text"
								class="input-bordered input input-sm"
								bind:value={modulePageEmptyStateAction}
							/>
						</div>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="empty-state-title"
							>Empty State Title</label
						>
						<input
							id="empty-state-title"
							type="text"
							class="input-bordered input input-sm"
							bind:value={modulePageEmptyStateTitle}
						/>
					</div>
					<div class="flex flex-col gap-1">
						<label class="text-xs font-semibold text-base-content/70" for="empty-state-description"
							>Empty State Description</label
						>
						<textarea
							id="empty-state-description"
							class="textarea-bordered textarea textarea-sm"
							bind:value={modulePageEmptyStateDescription}
							rows={2}
						></textarea>
					</div>
				</div>
			</div>

			<div class="flex items-center justify-end gap-3">
				<a href="/admin/modules" class="btn btn-ghost btn-sm">Cancel</a>
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
