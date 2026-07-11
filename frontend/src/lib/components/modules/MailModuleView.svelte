<script lang="ts">
	import { goto } from '$app/navigation';
	import { createQuery } from '$lib/query-compat';
	import { mailApi, type MailMessage } from '$lib/api/mail';
	import EmptyState from '$lib/components/common/EmptyState.svelte';
	import ModulePageSkeleton from '$lib/components/common/ModulePageSkeleton.svelte';
	import ErrorState from '$lib/components/common/ErrorState.svelte';
	import ModulePageShell from '$lib/components/layout/ModulePageShell.svelte';
	import { Mail, Download } from 'lucide-svelte';
	import type { ModuleDefinition } from '$lib/modules/registry';

	let { module }: { module: ModuleDefinition } = $props();

	const messagesQuery = createQuery({
		queryKey: ['mail-messages'],
		queryFn: () => mailApi.listMessages()
	});

	function handleOpenMessage(message: MailMessage) {
		goto(`/modules/mail/messages/${message.id}`);
	}

	function formatAddresses(value: unknown): string {
		if (Array.isArray(value)) return value.join(', ');
		return String(value ?? '');
	}
</script>

<ModulePageShell title="Mail" subtitle={module.ui.page.emptyStateDescription}>
	<div slot="primaryAction">
		<button class="btn gap-2 btn-sm btn-primary" onclick={() => goto('/files?folder=')}>
			<Download size={14} />
			<span>Import mail</span>
		</button>
	</div>

	{#if $messagesQuery.isLoading}
		<ModulePageSkeleton />
	{:else if $messagesQuery.isError}
		<ErrorState
			title="Failed to load mail"
			message={$messagesQuery.error?.message || 'Unknown error'}
			onRetry={() => $messagesQuery.refetch()}
		/>
	{:else if !$messagesQuery.data || $messagesQuery.data.length === 0}
		<EmptyState
			icon={'✉️'}
			title={module.ui.page.emptyStateTitle}
			description={module.ui.page.emptyStateDescription}
			actionLabel={module.ui.page.primaryAction?.label}
			onAction={() => goto('/files')}
		/>
	{:else}
		<div class="flex flex-col gap-2">
			{#each $messagesQuery.data as message}
				<button
					type="button"
					class="flex items-center gap-4 rounded-xl border border-base-300/50 bg-base-100 p-4 text-left shadow-sm transition-all hover:border-brand-500/40"
					onclick={() => handleOpenMessage(message)}
				>
					<div
						class="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-500"
					>
						<Mail size={22} />
					</div>
					<div class="flex min-w-0 flex-1 flex-col gap-1">
						<span class="truncate text-sm font-semibold text-base-content">
							{message.subject || '(no subject)'}
						</span>
						<span class="truncate text-xs text-base-content/55">
							{message.from_name || message.from_address || 'Unknown sender'}
							{#if message.sent_at}
								• {new Date(message.sent_at).toLocaleString()}
							{:else}
								• imported {new Date(message.imported_at).toLocaleString()}
							{/if}
						</span>
						<span class="truncate text-xs text-base-content/45">
							To: {formatAddresses(message.to_addresses)}
						</span>
					</div>
					{#if message.has_attachments}
						<span class="badge badge-sm badge-ghost">attachments</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}
</ModulePageShell>
