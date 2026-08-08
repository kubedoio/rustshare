<script lang="ts">
	import { page } from '$app/stores';
	import { getApplicationByRouteSlug } from '$lib/applications/registry';
	import { currentUser } from '$lib/stores/auth';
	import ApplicationPageRenderer from './ApplicationPageRenderer.svelte';
	import ApplicationPageSkeleton from '$lib/components/common/ApplicationPageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import OfflineBanner from '$lib/components/common/OfflineBanner.svelte';
	import { Info, ShieldAlert, Loader2 } from 'lucide-svelte';

	let key = $derived($page.params.key || '');
	let module = $derived(getApplicationByRouteSlug(key));
	let user = $derived($currentUser);
	let moduleResolved = $derived(module !== undefined);

	// Permissions check
	let canUse = $derived(module?.enabled && module?.permissions.workspaceMembersCanUse);
	let pageEnabled = $derived(module?.ui.page.enabled);
</script>

<svelte:head>
	<title>{module?.displayName ?? 'Application'} - RustShare</title>
</svelte:head>

{#if !module}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-base-200 p-4 text-base-content/40">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Application Not Found</h1>
		<p class="mt-2 text-base-content/60">The requested module does not exist in the registry.</p>
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
	</div>
{:else if !module.enabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-warning/10 p-4 text-warning">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Application Disabled</h1>
		<p class="mt-2 text-base-content/60">This module is currently disabled by the administrator.</p>
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
	</div>
{:else if !pageEnabled}
	<div class="flex h-[70vh] flex-col items-center justify-center text-center">
		<div class="mb-4 rounded-full bg-base-200 p-4 text-base-content/40">
			<Info size={48} />
		</div>
		<h1 class="text-2xl font-bold">Application Page Disabled</h1>
		<p class="mt-2 text-base-content/60">The WebUI surface for this module is disabled.</p>
		<a href="/dashboard" class="btn mt-6 btn-ghost">Return to Dashboard</a>
	</div>
{:else}
	<ApplicationPageRenderer {module} />
{/if}
