<script lang="ts">
	import { page } from '$app/stores';
	import { getApplicationByRouteSlug } from '$lib/applications/registry';
	import MailSettingsPanel from '$lib/settings/MailSettingsPanel.svelte';

	let slug = $derived($page.params.slug ?? '');
	let application = $derived(getApplicationByRouteSlug(slug));
	let setting = $derived(
		(application?.settings ?? []).find((item) => item.route === $page.url.pathname)
	);
</script>

<svelte:head>
	<title>{setting?.label ?? application?.displayName ?? 'Application'} settings</title>
</svelte:head>

{#if application && setting}
	<div class="mx-auto max-w-4xl">
		<div class="mb-6">
			<a href="/settings" class="text-sm text-base-content/60 hover:text-base-content">← Settings</a
			>
			<h1 class="mt-3 text-2xl font-semibold text-base-content">
				{setting.label ?? `${application.displayName} settings`}
			</h1>
			<p class="mt-1 text-base-content/60">{application.description}</p>
		</div>

		{#if application.id === 'io.elembra.mail'}
			<MailSettingsPanel />
		{:else}
			<div class="rounded-xl border border-base-300 bg-base-200 p-6">
				<p class="text-base-content/70">
					This Application has declared a settings Contribution. Its configuration is managed by the
					Application runtime.
				</p>
			</div>
		{/if}
	</div>
{:else}
	<div class="rounded-xl border border-base-300 bg-base-200 p-6">
		<h1 class="text-xl font-semibold text-base-content">Application settings unavailable</h1>
		<p class="mt-2 text-base-content/60">This settings Contribution is not available.</p>
	</div>
{/if}
