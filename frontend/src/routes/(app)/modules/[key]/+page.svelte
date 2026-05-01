<script lang="ts">
	import { page } from '$app/stores';
	import { getModuleByKey } from '$lib/modules/registry';
	import { currentUser } from '$lib/stores/auth';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import ModulePageRenderer from './ModulePageRenderer.svelte';
	import { Folder, ArrowRight, Info } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	$: key = $page.params.key || '';
	$: module = getModuleByKey(key);
	$: user = $currentUser;

	// Permissions check
	$: canUse = module?.enabled && module?.permissions.workspaceMembersCanUse;
	$: pageEnabled = module?.ui.page.enabled;

	async function handlePrimaryAction() {
		const action = module?.ui.page.primaryAction;
		if (!action) return;

		if (action.action === 'create-from-template' && action.template) {
			console.log('Creating from template:', action.template);
			// In a real implementation, this would call the template engine
		}
	}

	function handleOpenRootFolder() {
		if (!module?.rootPath) return;
		// We'll redirect to files with path if possible, or just files
		goto(`/files?path=${encodeURIComponent(module.rootPath)}`);
	}
</script>

<svelte:head>
	<title>{module?.displayName ?? 'Module'} - RustShare</title>
</svelte:head>

{#if !module}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-base-200 p-4 text-base-content/40">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Module Not Found</h1>
		<p class="mt-2 text-base-content/60">The requested module does not exist in the registry.</p>
		<a href="/dashboard" class="btn btn-ghost mt-6">Return to Dashboard</a>
	</div>
{:else if !module.enabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-warning/10 p-4 text-warning">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Module Disabled</h1>
		<p class="mt-2 text-base-content/60">This module is currently disabled by the administrator.</p>
		<a href="/dashboard" class="btn btn-ghost mt-6">Return to Dashboard</a>
	</div>
{:else if !pageEnabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-base-200 p-4 text-base-content/40">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Module Page Disabled</h1>
		<p class="mt-2 text-base-content/60">The WebUI surface for this module is disabled.</p>
		<a href="/dashboard" class="btn btn-ghost mt-6">Return to Dashboard</a>
	</div>
{:else}
	<div class="module-page-container">
		<!-- Module Header -->
		<header class="module-header rs-surface">
			<div class="header-main">
				<div class="module-identity">
					<div class="module-icon-wrap">
						<ModuleIcon name={module.ui.sidebar.icon} size={28} />
					</div>
					<div class="module-title-block">
						<div class="module-meta">
							<span class="root-path">{module.rootPath}</span>
						</div>
						<h1>{module.displayName}</h1>
						<p class="module-desc">{module.description}</p>
					</div>
				</div>

				<div class="header-actions">
					<button class="btn btn-outline btn-sm gap-2" on:click={handleOpenRootFolder}>
						<Folder size={14} />
						<span>Browse Files</span>
					</button>

					{#if module.ui.page.primaryAction}
						<button class="btn btn-primary btn-sm gap-2" on:click={handlePrimaryAction}>
							<span>{module.ui.page.primaryAction.label}</span>
							<ArrowRight size={14} />
						</button>
					{/if}
				</div>
			</div>
		</header>

		<!-- Content Area -->
		<main class="module-content">
			<ModulePageRenderer {module} />
		</main>
	</div>
{/if}

<style>
	.module-page-container {
		max-width: 1320px;
		margin: 0 auto;
		padding: 0 2rem 2.75rem;
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	.module-header {
		padding: 2rem;
		border-radius: var(--rs-radius-xl);
		background: var(--rs-surface-raised);
		border: 1px solid var(--rs-border);
	}

	.header-main {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 2rem;
	}

	.module-identity {
		display: flex;
		gap: 1.5rem;
		min-width: 0;
	}

	.module-icon-wrap {
		flex-shrink: 0;
		width: 3.5rem;
		height: 3.5rem;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--rs-brand-soft);
		color: var(--rs-brand);
		border-radius: 1.15rem;
		border: 1px solid color-mix(in oklab, var(--rs-brand) 20%, transparent);
	}

	.module-title-block {
		min-width: 0;
	}

	.module-meta {
		margin-bottom: 0.25rem;
	}

	.root-path {
		font-size: 0.72rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--rs-text-muted);
	}

	.module-title-block h1 {
		margin: 0;
		font-size: 1.85rem;
		font-weight: 800;
		color: var(--rs-text);
	}

	.module-desc {
		margin: 0.5rem 0 0;
		font-size: 0.95rem;
		color: var(--rs-text-soft);
		max-width: 600px;
	}

	.header-actions {
		display: flex;
		gap: 0.75rem;
		flex-shrink: 0;
	}

	.module-content {
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	@media (max-width: 1023px) {
		.header-main {
			flex-direction: column;
			gap: 1.5rem;
		}

		.header-actions {
			justify-content: flex-start;
		}
	}

	@media (max-width: 767px) {
		.module-page-container {
			padding: 0 1rem 2rem;
		}

		.module-header {
			padding: 1.5rem;
		}

		.module-identity {
			flex-direction: column;
			gap: 1rem;
		}

		.header-actions {
			flex-direction: column;
			width: 100%;
		}

		.header-actions button {
			width: 100%;
		}
	}
</style>
