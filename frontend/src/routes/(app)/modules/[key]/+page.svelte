<script lang="ts">
	import { page } from '$app/stores';
	import { modulesStore } from '$lib/modules/registry';
	import { currentUser } from '$lib/stores/auth';
	import ModuleIcon from '$lib/components/dashboard/ModuleIcon.svelte';
	import ModulePageRenderer from './ModulePageRenderer.svelte';
	import { Folder, ArrowRight, Info } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	$: key = $page.params.key || '';
	$: module = $modulesStore.find(m => m.key === key);
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
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
	</div>
{:else if !module.enabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-warning/10 p-4 text-warning">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Module Disabled</h1>
		<p class="mt-2 text-base-content/60">This module is currently disabled by the administrator.</p>
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
	</div>
{:else if !pageEnabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-base-200 p-4 text-base-content/40">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Module Page Disabled</h1>
		<p class="mt-2 text-base-content/60">The WebUI surface for this module is disabled.</p>
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
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
						<h1>{module.displayName}</h1>
						<p class="module-desc">{module.description}</p>
					</div>
				</div>

				<div class="header-actions">
					<button class="btn gap-2 btn-outline btn-sm" on:click={handleOpenRootFolder}>
						<Folder size={14} />
						<span>Browse Files</span>
					</button>

					{#if module.ui.page.primaryAction}
						<button class="btn gap-2 btn-sm btn-primary" on:click={handlePrimaryAction}>
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
		max-width: 1440px;
		margin: 0 auto;
		padding: 2rem;
		display: flex;
		flex-direction: row;
		align-items: flex-start;
		gap: 2.5rem;
	}

	.module-header {
		width: 25%;
		flex-shrink: 0;
		position: sticky;
		top: 2rem;
		padding: 1.5rem;
		border-radius: var(--rs-radius-lg);
		background: var(--rs-surface-raised);
		border: 1px solid var(--rs-border);
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
	}

	.header-main {
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.module-identity {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.module-icon-wrap {
		width: 3rem;
		height: 3rem;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--rs-brand-soft);
		color: var(--rs-brand);
		border-radius: 0.75rem;
		border: 1px solid color-mix(in oklab, var(--rs-brand) 20%, transparent);
	}

	.module-title-block h1 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 700;
		color: var(--rs-text);
		line-height: 1.3;
	}

	.module-desc {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--rs-text-soft);
		line-height: 1.5;
	}

	.header-actions {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding-top: 1rem;
		border-top: 1px solid var(--rs-border);
	}

	.header-actions :global(.btn) {
		width: 100%;
		justify-content: flex-start;
	}

	.module-content {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2rem;
	}

	@media (max-width: 1024px) {
		.module-page-container {
			flex-direction: column;
			padding: 1.5rem;
			gap: 1.5rem;
		}

		.module-header {
			width: 100%;
			position: static;
			padding: 1rem;
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
		}

		.header-main {
			flex-direction: row;
			align-items: center;
			gap: 1rem;
		}

		.module-identity {
			flex-direction: row;
			align-items: center;
		}

		.module-icon-wrap {
			width: 2.5rem;
			height: 2.5rem;
		}

		.module-title-block h1 {
			font-size: 1.125rem;
		}

		.module-desc {
			display: none;
		}

		.header-actions {
			flex-direction: row;
			padding-top: 0;
			border-top: none;
		}

		.header-actions :global(.btn) {
			width: auto;
		}
	}

	@media (max-width: 640px) {
		.module-page-container {
			padding: 1rem;
		}

		.header-actions {
			display: none;
		}
	}
</style>
