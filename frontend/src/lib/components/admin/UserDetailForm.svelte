<script lang="ts">
	import { createMutation } from '$lib/query-compat';
	import { goto } from '$app/navigation';
	import {
		updateAdminUser,
		disableAdminUser,
		enableAdminUser,
		deleteAdminUser,
		type AdminUserDetail
	} from '$lib/api/admin';

	export let user: AdminUserDetail;
	export let onRefresh: () => void = () => {};

	let email = user.email;
	let display_name = user.display_name;
	let is_admin = user.is_admin;
	let quota_gb =
		user.storage_quota_bytes > 0
			? String(Math.round(user.storage_quota_bytes / (1024 * 1024 * 1024)))
			: '';
	let password = '';
	let confirm_password = '';

	$: {
		email = user.email;
		display_name = user.display_name;
		is_admin = user.is_admin;
		quota_gb =
			user.storage_quota_bytes > 0
				? String(Math.round(user.storage_quota_bytes / (1024 * 1024 * 1024)))
				: '';
		password = '';
		confirm_password = '';
	}
	let errors: Record<string, string> = {};

	let confirmDisable = false;
	let confirmDelete = false;

	function validate(): boolean {
		errors = {};
		if (!email.trim()) errors.email = 'Email is required';
		else if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) errors.email = 'Invalid email format';
		if (quota_gb !== '' && (isNaN(Number(quota_gb)) || Number(quota_gb) < 0)) {
			errors.quota = 'Quota must be a non-negative number';
		}
		if (password) {
			if (password.length < 8) {
				errors.password = 'Password must be at least 8 characters';
			}
			if (password !== confirm_password) {
				errors.confirm_password = 'Passwords do not match';
			}
		}
		return Object.keys(errors).length === 0;
	}

	const updateMutation = createMutation({
		mutationFn: () =>
			updateAdminUser(user.id, {
				email: email.trim(),
				display_name: display_name.trim() || undefined,
				is_admin,
				storage_quota_bytes:
					quota_gb !== '' ? Math.round(Number(quota_gb) * 1024 * 1024 * 1024) : undefined,
				password: password || undefined
			}),
		onSuccess: () => {
			password = '';
			confirm_password = '';
			onRefresh();
		}
	});

	const disableMutation = createMutation({
		mutationFn: () => disableAdminUser(user.id),
		onSuccess: () => {
			confirmDisable = false;
			onRefresh();
		}
	});

	const enableMutation = createMutation({
		mutationFn: () => enableAdminUser(user.id),
		onSuccess: () => onRefresh()
	});

	const deleteMutation = createMutation({
		mutationFn: () => deleteAdminUser(user.id),
		onSuccess: () => {
			confirmDelete = false;
			goto('/admin/users');
		}
	});

	function handleSubmit() {
		if (!validate()) return;
		$updateMutation.mutate();
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return '0 bytes';
		const gb = bytes / (1024 * 1024 * 1024);
		return gb >= 1 ? `${gb.toFixed(2)} GB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}
</script>

<div class="space-y-6">
	<!-- User info header -->
	<div class="card bg-base-100 shadow">
		<div class="card-body">
			<div class="flex flex-wrap items-start justify-between gap-3">
				<div>
					<h2 class="text-xl font-bold">{user.username}</h2>
					<p class="text-sm text-base-content/60">{user.email}</p>
					<div class="mt-2 flex gap-2">
						{#if user.disabled_at}
							<span class="badge badge-error">Disabled</span>
						{:else}
							<span class="badge badge-success">Active</span>
						{/if}
						{#if user.is_admin}
							<span class="badge badge-warning">Admin</span>
						{/if}
					</div>
				</div>
				<div class="text-right text-sm text-base-content/60">
					<p>Storage used: {formatBytes(user.storage_used_bytes)}</p>
					<p>Joined: {new Date(user.created_at).toLocaleDateString()}</p>
				</div>
			</div>
		</div>
	</div>

	<!-- Edit form -->
	<div class="card bg-base-100 shadow">
		<div class="card-body">
			<h3 class="card-title text-base">Edit Details</h3>
			<form on:submit|preventDefault={handleSubmit} class="mt-2 space-y-4">
				<div class="form-control">
					<label class="label" for="edit-email"><span class="label-text">Email</span></label>
					<input
						id="edit-email"
						type="email"
						class="input-bordered input"
						class:input-error={errors.email}
						bind:value={email}
					/>
					{#if errors.email}<p class="mt-1 text-xs text-error">{errors.email}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="edit-display-name">
						<span class="label-text">Display Name</span>
					</label>
					<input
						id="edit-display-name"
						type="text"
						class="input-bordered input"
						bind:value={display_name}
					/>
				</div>

				<div class="form-control">
					<label class="label" for="edit-quota">
						<span class="label-text">Storage Quota (GB, blank = unlimited)</span>
					</label>
					<input
						id="edit-quota"
						type="number"
						min="0"
						step="0.1"
						class="input-bordered input"
						class:input-error={errors.quota}
						bind:value={quota_gb}
						placeholder="Unlimited"
					/>
					{#if errors.quota}<p class="mt-1 text-xs text-error">{errors.quota}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="checkbox" bind:checked={is_admin} />
						<span class="label-text">Admin privileges</span>
					</label>
				</div>

				<div class="divider"></div>
				<h4 class="text-sm font-semibold">Set Password</h4>
				<p class="text-xs text-base-content/60">
					Leave blank to keep the current password. Changing the password will log the user out
					everywhere.
				</p>

				<div class="form-control mt-2">
					<label class="label" for="edit-password">
						<span class="label-text">New Password</span>
					</label>
					<input
						id="edit-password"
						type="password"
						class="input-bordered input"
						class:input-error={errors.password}
						bind:value={password}
						placeholder="••••••••"
					/>
					{#if errors.password}<p class="mt-1 text-xs text-error">{errors.password}</p>{/if}
				</div>

				<div class="form-control">
					<label class="label" for="edit-confirm-password">
						<span class="label-text">Confirm Password</span>
					</label>
					<input
						id="edit-confirm-password"
						type="password"
						class="input-bordered input"
						class:input-error={errors.confirm_password}
						bind:value={confirm_password}
						placeholder="••••••••"
					/>
					{#if errors.confirm_password}<p class="mt-1 text-xs text-error">
							{errors.confirm_password}
						</p>{/if}
				</div>

				{#if $updateMutation.isError}
					<div class="alert text-sm alert-error">
						{$updateMutation.error instanceof Error
							? $updateMutation.error.message
							: 'Failed to update user'}
					</div>
				{/if}
				{#if $updateMutation.isSuccess}
					<div class="alert text-sm alert-success">User updated successfully.</div>
				{/if}

				<div class="flex gap-2">
					<button type="submit" class="btn btn-primary" disabled={$updateMutation.isPending}>
						{$updateMutation.isPending ? 'Saving...' : 'Save Changes'}
					</button>
				</div>
			</form>
		</div>
	</div>

	<!-- Danger zone -->
	<div class="card border border-error/30 bg-base-100 shadow">
		<div class="card-body">
			<h3 class="card-title text-base text-error">Danger Zone</h3>
			<div class="mt-2 flex flex-wrap gap-3">
				{#if user.disabled_at}
					<button
						class="btn btn-outline btn-sm btn-success"
						on:click={() => $enableMutation.mutate()}
						disabled={$enableMutation.isPending}
					>
						{$enableMutation.isPending ? 'Enabling...' : 'Enable Account'}
					</button>
				{:else}
					<button
						class="btn btn-outline btn-sm btn-warning"
						on:click={() => (confirmDisable = true)}
					>
						Disable Account
					</button>
				{/if}
				<button class="btn btn-outline btn-sm btn-error" on:click={() => (confirmDelete = true)}>
					Delete User
				</button>
			</div>
		</div>
	</div>
</div>

<!-- Disable confirmation -->
{#if confirmDisable}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Disable Account</h3>
			<p class="py-4">
				Are you sure you want to disable <strong>{user.username}</strong>? They will be unable to
				log in.
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmDisable = false)}>Cancel</button>
				<button
					class="btn btn-warning"
					on:click={() => $disableMutation.mutate()}
					disabled={$disableMutation.isPending}
				>
					{$disableMutation.isPending ? 'Disabling...' : 'Disable'}
				</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmDisable = false)} role="presentation"></div>
	</div>
{/if}

<!-- Delete confirmation -->
{#if confirmDelete}
	<div class="modal-open modal">
		<div class="modal-box">
			<h3 class="text-lg font-bold">Delete User</h3>
			<p class="py-4">
				Are you sure you want to permanently delete <strong>{user.username}</strong>? This action
				cannot be undone.
			</p>
			<div class="modal-action">
				<button class="btn btn-ghost" on:click={() => (confirmDelete = false)}>Cancel</button>
				<button
					class="btn btn-error"
					on:click={() => $deleteMutation.mutate()}
					disabled={$deleteMutation.isPending}
				>
					{$deleteMutation.isPending ? 'Deleting...' : 'Delete'}
				</button>
			</div>
		</div>
		<div class="modal-backdrop" on:click={() => (confirmDelete = false)} role="presentation"></div>
	</div>
{/if}
