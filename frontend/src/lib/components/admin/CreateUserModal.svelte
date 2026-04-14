<script lang="ts">
	import { createMutation } from '$lib/query-compat';
	import { createAdminUser, type AdminUserDetail } from '$lib/api/admin';

	export let open: boolean = false;
	export let onClose: () => void = () => {};
	export let onCreated: (user: AdminUserDetail) => void = () => {};

	let username = '';
	let email = '';
	let password = '';
	let display_name = '';
	let is_admin = false;
	let quota_gb = '';
	let errors: Record<string, string> = {};

	function validate(): boolean {
		errors = {};
		if (!username.trim()) errors.username = 'Username is required';
		else if (username.length < 3) errors.username = 'Username must be at least 3 characters';
		if (!email.trim()) errors.email = 'Email is required';
		else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) errors.email = 'Invalid email format';
		if (!password) errors.password = 'Password is required';
		else if (password.length < 8) errors.password = 'Password must be at least 8 characters';
		if (quota_gb !== '' && (isNaN(Number(quota_gb)) || Number(quota_gb) < 0)) {
			errors.quota = 'Quota must be a non-negative number';
		}
		return Object.keys(errors).length === 0;
	}

	const mutation = createMutation({
		mutationFn: () =>
			createAdminUser({
				username: username.trim(),
				email: email.trim(),
				password,
				display_name: display_name.trim() || undefined,
				is_admin,
				storage_quota_bytes:
					quota_gb !== '' ? Math.round(Number(quota_gb) * 1024 * 1024 * 1024) : undefined
			}),
		onSuccess: (user) => {
			onCreated(user);
			resetForm();
		}
	});

	function handleSubmit() {
		if (!validate()) return;
		$mutation.mutate();
	}

	function resetForm() {
		username = '';
		email = '';
		password = '';
		display_name = '';
		is_admin = false;
		quota_gb = '';
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
			<h3 class="font-bold text-lg mb-4">Create User</h3>

			<form on:submit|preventDefault={handleSubmit} class="space-y-4">
				<div class="form-control">
					<label class="label" for="username"><span class="label-text">Username *</span></label>
					<input
						id="username"
						type="text"
						class="input input-bordered"
						class:input-error={errors.username}
						bind:value={username}
						autocomplete="off"
					/>
					{#if errors.username}<p class="text-error text-xs mt-1">{errors.username}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="email"><span class="label-text">Email *</span></label>
					<input
						id="email"
						type="email"
						class="input input-bordered"
						class:input-error={errors.email}
						bind:value={email}
						autocomplete="off"
					/>
					{#if errors.email}<p class="text-error text-xs mt-1">{errors.email}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="password"><span class="label-text">Password *</span></label>
					<input
						id="password"
						type="password"
						class="input input-bordered"
						class:input-error={errors.password}
						bind:value={password}
						autocomplete="new-password"
					/>
					{#if errors.password}<p class="text-error text-xs mt-1">{errors.password}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="display_name"><span class="label-text">Display Name</span></label>
					<input
						id="display_name"
						type="text"
						class="input input-bordered"
						bind:value={display_name}
					/>
				</div>

				<div class="form-control">
					<label class="label" for="quota_gb">
						<span class="label-text">Storage Quota (GB, leave blank for unlimited)</span>
					</label>
					<input
						id="quota_gb"
						type="number"
						min="0"
						step="0.1"
						class="input input-bordered"
						class:input-error={errors.quota}
						bind:value={quota_gb}
						placeholder="e.g. 10"
					/>
					{#if errors.quota}<p class="text-error text-xs mt-1">{errors.quota}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="checkbox" bind:checked={is_admin} />
						<span class="label-text">Grant admin privileges</span>
					</label>
				</div>

				{#if $mutation.isError}
					<div class="alert alert-error text-sm">
						{$mutation.error instanceof Error ? $mutation.error.message : 'Failed to create user'}
					</div>
				{/if}

				<div class="modal-action">
					<button type="button" class="btn btn-ghost" on:click={handleClose}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={$mutation.isPending}>
						{$mutation.isPending ? 'Creating...' : 'Create User'}
					</button>
				</div>
			</form>
		</div>
		<div class="modal-backdrop" on:click={handleClose} role="presentation"></div>
	</div>
{/if}
