<script lang="ts">
	import { createMutation } from '@tanstack/svelte-query';
	import { updateWebhook, deleteWebhook, testWebhook, type Webhook } from '$lib/api/admin';

	export let webhooks: Webhook[] = [];
	export let onRefresh: () => void = () => {};
	export let onCreate: () => void = () => {};

	let confirmDelete: string | null = null;
	let testResults: Record<string, { success: boolean; message?: string }> = {};

	const toggleMutation = createMutation({
		mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
			updateWebhook(id, { enabled }),
		onSuccess: () => onRefresh()
	});

	const deleteMutation = createMutation({
		mutationFn: (id: string) => deleteWebhook(id),
		onSuccess: () => {
			confirmDelete = null;
			onRefresh();
		}
	});

	const testMutation = createMutation({
		mutationFn: (id: string) => testWebhook(id),
		onSuccess: (res, id) => {
			testResults = { ...testResults, [id]: res };
		},
		onError: (err, id) => {
			testResults = {
				...testResults,
				[id]: { success: false, message: err instanceof Error ? err.message : 'Test failed' }
			};
		}
	});

	const EVENT_LABELS: Record<string, string> = {
		file_uploaded: 'File Uploaded',
		file_deleted: 'File Deleted',
		file_shared: 'File Shared',
		user_created: 'User Created',
		user_deleted: 'User Deleted'
	};
</script>

<div class="space-y-4">
	<div class="flex justify-between items-center">
		<span class="text-sm text-base-content/60">
			{webhooks.length} webhook{webhooks.length !== 1 ? 's' : ''}
		</span>
		<button class="btn btn-primary btn-sm" on:click={onCreate}>Add Webhook</button>
	</div>

	{#if webhooks.length === 0}
		<div class="text-center py-12 text-base-content/50">
			<p class="text-lg">No webhooks configured</p>
			<p class="text-sm mt-1">Add a webhook to receive HTTP notifications for events.</p>
		</div>
	{/if}

	{#each webhooks as wh (wh.id)}
		<div class="card bg-base-100 shadow border border-base-300">
			<div class="card-body p-4">
				<div class="flex items-start justify-between flex-wrap gap-3">
					<div class="flex-1 min-w-0">
						<div class="flex items-center gap-2 flex-wrap">
							<h4 class="font-semibold">{wh.name}</h4>
							{#if wh.enabled}
								<span class="badge badge-success badge-sm">Enabled</span>
							{:else}
								<span class="badge badge-ghost badge-sm">Disabled</span>
							{/if}
						</div>
						<p class="text-sm text-base-content/60 mt-1 break-all">{wh.url}</p>
						<div class="flex flex-wrap gap-1 mt-2">
							{#each wh.events as event}
								<span class="badge badge-outline badge-xs">
									{EVENT_LABELS[event] ?? event}
								</span>
							{/each}
						</div>
					</div>

					<div class="flex items-center gap-2 flex-shrink-0">
						<input
							type="checkbox"
							class="toggle toggle-sm toggle-success"
							checked={wh.enabled}
							on:change={(e) =>
								$toggleMutation.mutate({ id: wh.id, enabled: (e.target as HTMLInputElement).checked })}
						/>
						<button
							class="btn btn-ghost btn-xs"
							on:click={() => $testMutation.mutate(wh.id)}
							disabled={$testMutation.isPending}
						>
							Test
						</button>
						<button
							class="btn btn-ghost btn-xs text-error"
							on:click={() => (confirmDelete = wh.id)}
						>
							Delete
						</button>
					</div>
				</div>

				{#if testResults[wh.id]}
					<div
						class="alert alert-sm mt-2 text-xs"
						class:alert-success={testResults[wh.id].success}
						class:alert-error={!testResults[wh.id].success}
					>
						{testResults[wh.id].message ??
							(testResults[wh.id].success ? 'Test successful' : 'Test failed')}
					</div>
				{/if}
			</div>
		</div>
	{/each}
</div>

{#if confirmDelete}
	<div class="modal modal-open">
		<div class="modal-box">
			<h3 class="font-bold text-lg">Delete Webhook</h3>
			<p class="py-4">Are you sure you want to delete this webhook?</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmDelete = null)}>Cancel</button>
				<button
					class="btn btn-error"
					on:click={() => confirmDelete && $deleteMutation.mutate(confirmDelete)}
					disabled={$deleteMutation.isPending}
				>
					{$deleteMutation.isPending ? 'Deleting...' : 'Delete'}
				</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmDelete = null)} role="presentation"></div>
	</div>
{/if}
