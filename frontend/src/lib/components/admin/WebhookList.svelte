<script lang="ts">
	import { createMutation } from '$lib/query-compat';
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
		'file.uploaded': 'File Uploaded',
		'file.deleted': 'File Deleted',
		'file.restored': 'File Restored',
		'folder.created': 'Folder Created',
		'folder.deleted': 'Folder Deleted',
		'share.created': 'Share Created',
		'share.revoked': 'Share Revoked',
		'user.created': 'User Created',
		'user.disabled': 'User Disabled',
		'user.deleted': 'User Deleted'
	};
</script>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<span class="text-sm text-base-content/60">
			{webhooks.length} webhook{webhooks.length !== 1 ? 's' : ''}
		</span>
		<button class="btn btn-sm btn-primary" on:click={onCreate}>Add Webhook</button>
	</div>

	{#if webhooks.length === 0}
		<div class="py-12 text-center text-base-content/50">
			<p class="text-lg">No webhooks configured</p>
			<p class="mt-1 text-sm">Add a webhook to receive HTTP notifications for events.</p>
		</div>
	{/if}

	{#each webhooks as wh (wh.id)}
		<div class="card border border-base-300 bg-base-100 shadow">
			<div class="card-body p-4">
				<div class="flex flex-wrap items-start justify-between gap-3">
					<div class="min-w-0 flex-1">
						<div class="flex flex-wrap items-center gap-2">
							<h4 class="font-semibold">{wh.name}</h4>
							{#if wh.enabled}
								<span class="badge badge-sm badge-success">Enabled</span>
							{:else}
								<span class="badge badge-ghost badge-sm">Disabled</span>
							{/if}
						</div>
						<p class="mt-1 text-sm break-all text-base-content/60">{wh.url}</p>
						<div class="mt-2 flex flex-wrap gap-1">
							{#each wh.events as event}
								<span class="badge badge-outline badge-xs">
									{EVENT_LABELS[event] ?? event}
								</span>
							{/each}
						</div>
					</div>

					<div class="flex flex-shrink-0 items-center gap-2">
						<input
							type="checkbox"
							class="toggle toggle-sm toggle-success"
							checked={wh.enabled}
							on:change={(e) =>
								$toggleMutation.mutate({
									id: wh.id,
									enabled: (e.target as HTMLInputElement).checked
								})}
						/>
						<button
							class="btn btn-ghost btn-xs"
							on:click={() => $testMutation.mutate(wh.id)}
							disabled={$testMutation.isPending}
						>
							Test
						</button>
						<button
							class="btn text-error btn-ghost btn-xs"
							on:click={() => (confirmDelete = wh.id)}
						>
							Delete
						</button>
					</div>
				</div>

				{#if testResults[wh.id]}
					<div
						class="alert-sm mt-2 alert text-xs"
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
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Delete Webhook</h3>
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
