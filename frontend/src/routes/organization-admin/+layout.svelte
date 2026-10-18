<script lang="ts">
	import { onMount } from 'svelte';
	import { getOrganizationAdminCapability } from '$lib/api/organization-admin';

	let state = $state<'loading' | 'allowed' | 'denied' | 'error'>('loading');

	onMount(() => {
		let active = true;
		const checkCapability = async () => {
			try {
				const result = await getOrganizationAdminCapability();
				if (active) state = result.allowed ? 'allowed' : 'denied';
			} catch (error: unknown) {
				if (active)
					state =
						error instanceof Error && 'status' in error && error.status === 403
							? 'denied'
							: 'error';
			}
		};
		const checkWhenVisible = () => {
			if (!document.hidden) void checkCapability();
		};
		void checkCapability();
		window.addEventListener('focus', checkCapability);
		document.addEventListener('visibilitychange', checkWhenVisible);
		return () => {
			active = false;
			window.removeEventListener('focus', checkCapability);
			document.removeEventListener('visibilitychange', checkWhenVisible);
		};
	});
</script>

<svelte:head>
	<title>Organization administration — RustShare</title>
</svelte:head>

{#if state === 'loading'}
	<main class="flex min-h-64 items-center justify-center" aria-live="polite">
		<span class="loading loading-lg loading-spinner" aria-label="Checking organization access"
		></span>
	</main>
{:else if state === 'denied'}
	<main class="mx-auto max-w-2xl p-8" role="alert">
		<h1 class="text-2xl font-bold">Organization administration unavailable</h1>
		<p class="mt-2">This account is not authorized to administer an organization.</p>
		<a class="btn mt-4 btn-ghost" href="/dashboard">Return to workspace</a>
	</main>
{:else if state === 'error'}
	<main class="mx-auto max-w-2xl p-8" role="alert">
		<h1 class="text-2xl font-bold">Could not verify organization access</h1>
		<p class="mt-2">Retry after checking your connection.</p>
		<button class="btn mt-4 btn-primary" onclick={() => location.reload()}>Retry</button>
	</main>
{:else}
	<div class="mx-auto max-w-7xl p-4 sm:p-6">
		<header class="mb-6 border-b border-base-300 pb-4">
			<p class="text-sm text-base-content/60">RustShare</p>
			<h1 class="text-3xl font-bold">Organization administration</h1>
			<p class="mt-1 text-sm text-base-content/70">
				Manage users, groups, invitations and administrative activity for your organization.
			</p>
		</header>
		<nav class="mb-6 flex flex-wrap gap-2" aria-label="Organization administration">
			<a class="btn btn-sm" href="/organization-admin/users">Users</a>
			<a class="btn btn-sm" href="/organization-admin/groups">Groups</a>
			<a class="btn btn-sm" href="/organization-admin/invitations">Invitations</a>
			<a class="btn btn-sm" href="/organization-admin/audit">Audit</a>
		</nav>
		<slot />
		<p class="mt-8 border-t border-base-300 pt-3 text-xs text-base-content/60">
			Independent workspaces and workspace membership administration are deferred under ADR-0040.
		</p>
	</div>
{/if}
