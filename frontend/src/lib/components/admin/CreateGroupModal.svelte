<script lang="ts">
	import { createMutation } from '$lib/query-compat';
	import { createAdminGroup, type AdminGroupDetail } from '$lib/api/admin';

	export let open: boolean = false;
	export let onClose: () => void = () => {};
	export let onCreated: (group: AdminGroupDetail) => void = () => {};

	let name = '';
	let description = '';
	let errors: Record<string, string> = {};

	function validate(): boolean {
		errors = {};
		if (!name.trim()) errors.name = 'Group name is required';
		else if (name.length < 2) errors.name = 'Name must be at least 2 characters';
		return Object.keys(errors).length === 0;
	}

	const mutation = createMutation({
		mutationFn: () =>
			createAdminGroup({
				name: name.trim(),
				description: description.trim() || undefined
			}),
		onSuccess: (group) => {
			onCreated(group);
			resetForm();
		}
	});

	function handleSubmit() {
		if (!validate()) return;
		$mutation.mutate();
	}

	function resetForm() {
		name = '';
		description = '';
		errors = {};
	}

	function handleClose() {
		resetForm();
		onClose();
	}
</script>

{#if open}
	<div class="modal-open modal">
		<div class="modal-box w-full max-w-md">
			<h3 class="mb-4 text-lg font-bold">Create Group</h3>

			<form on:submit|preventDefault={handleSubmit} class="space-y-4">
				<div class="form-control">
					<label class="label" for="group-name"><span class="label-text">Name *</span></label>
					<input
						id="group-name"
						type="text"
						class="input-bordered input"
						class:input-error={errors.name}
						bind:value={name}
					/>
					{#if errors.name}<p class="mt-1 text-xs text-error">{errors.name}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="group-description">
						<span class="label-text">Description</span>
					</label>
					<textarea
						id="group-description"
						class="textarea-bordered textarea"
						rows="3"
						bind:value={description}
						placeholder="Optional description..."
					></textarea>
				</div>

				{#if $mutation.isError}
					<div class="alert text-sm alert-error">
						{$mutation.error instanceof Error ? $mutation.error.message : 'Failed to create group'}
					</div>
				{/if}

				<div class="modal-action">
					<button type="button" class="btn btn-ghost" on:click={handleClose}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={$mutation.isPending}>
						{$mutation.isPending ? 'Creating...' : 'Create Group'}
					</button>
				</div>
			</form>
		</div>
		<div class="modal-backdrop" on:click={handleClose} role="presentation"></div>
	</div>
{/if}
