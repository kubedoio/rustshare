<script lang="ts">
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import {
		getSecurityConfig,
		updateSecurityConfig,
		type SecurityConfigRequest
	} from '$lib/api/admin';

	const queryClient = useQueryClient();

	const query = createQuery({
		queryKey: ['admin', 'security-config'],
		queryFn: getSecurityConfig
	});

	let loginProtectionEnabled = $state(true);
	let maxLoginAttempts = $state(5);
	let blockDurationMinutes = $state(15);

	$effect(() => {
		if ($query.data) {
			loginProtectionEnabled = $query.data.login_protection_enabled;
			maxLoginAttempts = $query.data.max_login_attempts;
			blockDurationMinutes = $query.data.login_block_duration_minutes;
		}
	});

	const saveMutation = createMutation({
		mutationFn: () => {
			const data: SecurityConfigRequest = {
				login_protection_enabled: loginProtectionEnabled,
				max_login_attempts: maxLoginAttempts,
				login_block_duration_minutes: blockDurationMinutes
			};
			return updateSecurityConfig(data);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'security-config'] });
		}
	});
</script>

<svelte:head>
	<title>Security — Admin | RustShare</title>
</svelte:head>

<div class="max-w-3xl space-y-4">
	<h2 class="text-2xl font-bold">Security</h2>

	<div class="card border border-base-300/80 bg-base-100 shadow">
		<div class="card-body gap-6">
			<div>
				<h3 class="card-title text-xl">Login Protection</h3>
				<p class="mt-2 text-sm leading-6 text-base-content/70">
					Configure brute-force attack protection for the login page. After the configured number of
					consecutive failed attempts from a single IP address, that IP will be temporarily blocked
					from logging in.
				</p>
			</div>

			{#if $query.isLoading}
				<div class="flex justify-center py-10" aria-live="polite">
					<span class="loading loading-md loading-spinner"></span>
				</div>
			{:else if $query.isError}
				<div class="alert alert-error" role="alert">
					<span>Failed to load security settings.</span>
				</div>
			{:else}
				<form
					onsubmit={(e) => {
						e.preventDefault();
						$saveMutation.mutate();
					}}
					class="space-y-6"
				>
					<section class="rounded-[1.4rem] border border-base-300/80 bg-base-200/35 p-5">
						<div class="flex items-start justify-between gap-4">
							<div>
								<p class="text-sm font-medium text-base-content">Enable login protection</p>
								<p class="mt-1 text-xs text-base-content/60">
									Track failed login attempts and block IPs after too many failures.
								</p>
							</div>
							<label class="label cursor-pointer gap-3">
								<input
									type="checkbox"
									class="toggle toggle-primary"
									bind:checked={loginProtectionEnabled}
								/>
							</label>
						</div>
					</section>

					<section class="grid gap-4 md:grid-cols-2">
						<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
							<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
								Max Failed Attempts
							</p>
							<p class="mt-2 text-sm text-base-content/70">
								Number of consecutive failed login attempts before an IP is blocked.
							</p>
							<div class="mt-4">
								<input
									type="number"
									min="1"
									max="100"
									class="input-bordered input w-full"
									bind:value={maxLoginAttempts}
									disabled={!loginProtectionEnabled}
								/>
							</div>
						</div>

						<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
							<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
								Block Duration
							</p>
							<p class="mt-2 text-sm text-base-content/70">
								How long (in minutes) an IP remains blocked after reaching the limit.
							</p>
							<div class="mt-4">
								<input
									type="number"
									min="1"
									max="10080"
									class="input-bordered input w-full"
									bind:value={blockDurationMinutes}
									disabled={!loginProtectionEnabled}
								/>
							</div>
						</div>
					</section>

					{#if $saveMutation.isError}
						<div class="alert text-sm alert-error" role="alert">
							{$saveMutation.error instanceof Error
								? $saveMutation.error.message
								: 'Failed to save security configuration'}
						</div>
					{/if}

					{#if $saveMutation.isSuccess}
						<div class="alert text-sm alert-success" role="status">
							Security settings saved successfully.
						</div>
					{/if}

					<div class="flex gap-3">
						<button type="submit" class="btn btn-primary" disabled={$saveMutation.isPending}>
							{$saveMutation.isPending ? 'Saving...' : 'Save security settings'}
						</button>
					</div>
				</form>
			{/if}
		</div>
	</div>
</div>
