<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { queryClient } from '$lib/query-client';
	import { listWebhooks } from '$lib/api/admin';
	import WebhookList from '$lib/components/admin/WebhookList.svelte';
	import CreateWebhookModal from '$lib/components/admin/CreateWebhookModal.svelte';
	import SmtpConfigForm from '$lib/components/admin/SmtpConfigForm.svelte';

	let activeTab: 'webhooks' | 'smtp' = 'webhooks';
	let showCreateWebhookModal = false;

	const webhooksQuery = createQuery({
		queryKey: ['admin', 'webhooks'],
		queryFn: listWebhooks
	});

	function handleRefreshWebhooks() {
		queryClient.invalidateQueries({ queryKey: ['admin', 'webhooks'] });
	}

	function handleWebhookCreated() {
		showCreateWebhookModal = false;
		handleRefreshWebhooks();
	}
</script>

<svelte:head>
	<title>Integrations — Admin | RustShare</title>
</svelte:head>

<div class="space-y-4">
	<h2 class="text-2xl font-bold">Integrations</h2>

	<!-- Tabs -->
	<div class="tabs tabs-bordered">
		<button
			class="tab"
			class:tab-active={activeTab === 'webhooks'}
			on:click={() => (activeTab = 'webhooks')}
		>
			Webhooks
		</button>
		<button
			class="tab"
			class:tab-active={activeTab === 'smtp'}
			on:click={() => (activeTab = 'smtp')}
		>
			SMTP Email
		</button>
	</div>

	{#if activeTab === 'webhooks'}
		{#if $webhooksQuery.isLoading}
			<div class="flex justify-center py-16">
				<span class="loading loading-spinner loading-lg"></span>
			</div>
		{:else if $webhooksQuery.isError}
			<div class="alert alert-error">
				Failed to load webhooks: {$webhooksQuery.error instanceof Error ? $webhooksQuery.error.message : 'Unknown error'}
			</div>
		{:else if $webhooksQuery.data}
			<WebhookList
				webhooks={$webhooksQuery.data.webhooks}
				onRefresh={handleRefreshWebhooks}
				onCreate={() => (showCreateWebhookModal = true)}
			/>
		{/if}
	{:else}
		<SmtpConfigForm />
	{/if}
</div>

<CreateWebhookModal
	open={showCreateWebhookModal}
	onClose={() => (showCreateWebhookModal = false)}
	onCreated={handleWebhookCreated}
/>
