<script lang="ts">
	import { createMutation } from '$lib/query-compat';
	import { createWebhook, type Webhook } from '$lib/api/admin';

	export let open: boolean = false;
	export let onClose: () => void = () => {};
	export let onCreated: (webhook: Webhook) => void = () => {};

	const ALL_EVENTS = [
		{ value: 'file.uploaded', label: 'File Uploaded' },
		{ value: 'file.deleted', label: 'File Deleted' },
		{ value: 'file.restored', label: 'File Restored' },
		{ value: 'folder.created', label: 'Folder Created' },
		{ value: 'folder.deleted', label: 'Folder Deleted' },
		{ value: 'share.created', label: 'Share Created' },
		{ value: 'share.revoked', label: 'Share Revoked' },
		{ value: 'user.created', label: 'User Created' },
		{ value: 'user.disabled', label: 'User Disabled' },
		{ value: 'user.deleted', label: 'User Deleted' }
	];

	let name = '';
	let url = '';
	let secret = '';
	let selectedEvents: string[] = [];
	let errors: Record<string, string> = {};

	function validate(): boolean {
		errors = {};
		if (!name.trim()) errors.name = 'Name is required';
		if (!url.trim()) errors.url = 'URL is required';
		else {
			try {
				new URL(url.trim());
			} catch {
				errors.url = 'Invalid URL format';
			}
		}
		if (selectedEvents.length === 0) errors.events = 'Select at least one event';
		return Object.keys(errors).length === 0;
	}

	const mutation = createMutation({
		mutationFn: () =>
			createWebhook({
				name: name.trim(),
				url: url.trim(),
				secret: secret.trim() || undefined,
				events: selectedEvents
			}),
		onSuccess: (wh) => {
			onCreated(wh);
			resetForm();
		}
	});

	function handleSubmit() {
		if (!validate()) return;
		$mutation.mutate();
	}

	function toggleEvent(value: string) {
		if (selectedEvents.includes(value)) {
			selectedEvents = selectedEvents.filter((e) => e !== value);
		} else {
			selectedEvents = [...selectedEvents, value];
		}
	}

	function resetForm() {
		name = '';
		url = '';
		secret = '';
		selectedEvents = [];
		errors = {};
	}

	function handleClose() {
		resetForm();
		onClose();
	}
</script>

{#if open}
	<div class="modal modal-open">
		<div class="modal-box w-full max-w-md">
			<h3 class="font-bold text-lg mb-4">Add Webhook</h3>

			<form on:submit|preventDefault={handleSubmit} class="space-y-4">
				<div class="form-control">
					<label class="label" for="wh-name"><span class="label-text">Name *</span></label>
					<input
						id="wh-name"
						type="text"
						class="input input-bordered"
						class:input-error={errors.name}
						bind:value={name}
						placeholder="My Webhook"
					/>
					{#if errors.name}<p class="text-error text-xs mt-1">{errors.name}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="wh-url"><span class="label-text">Endpoint URL *</span></label>
					<input
						id="wh-url"
						type="url"
						class="input input-bordered"
						class:input-error={errors.url}
						bind:value={url}
						placeholder="https://example.com/webhook"
					/>
					{#if errors.url}<p class="text-error text-xs mt-1">{errors.url}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="wh-secret">
						<span class="label-text">Signing Secret (optional)</span>
					</label>
					<input
						id="wh-secret"
						type="text"
						class="input input-bordered"
						bind:value={secret}
						placeholder="Leave blank to skip signing"
					/>
				</div>

				<div class="form-control">
					<span class="label-text font-medium mb-2 block">Events *</span>
					<div class="space-y-2">
						{#each ALL_EVENTS as event}
							<label class="label cursor-pointer justify-start gap-3">
								<input
									type="checkbox"
									class="checkbox checkbox-sm"
									checked={selectedEvents.includes(event.value)}
									on:change={() => toggleEvent(event.value)}
								/>
								<span class="label-text">{event.label}</span>
							</label>
						{/each}
					</div>
					{#if errors.events}<p class="text-error text-xs mt-1">{errors.events}</p>{/if}
				</div>

				{#if $mutation.isError}
					<div class="alert alert-error text-sm">
						{$mutation.error instanceof Error ? $mutation.error.message : 'Failed to create webhook'}
					</div>
				{/if}

				<div class="modal-action">
					<button type="button" class="btn btn-ghost" on:click={handleClose}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={$mutation.isPending}>
						{$mutation.isPending ? 'Creating...' : 'Add Webhook'}
					</button>
				</div>
			</form>
		</div>
		<div class="modal-backdrop" on:click={handleClose} role="presentation"></div>
	</div>
{/if}
