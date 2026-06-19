<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { getSmtpConfig, updateSmtpConfig, type SmtpConfigRequest } from '$lib/api/admin';

	const queryClient = useQueryClient();

	const query = createQuery({
		queryKey: ['admin', 'smtp-config'],
		queryFn: getSmtpConfig
	});

	let enabled = $state(false);
	let host = $state('');
	let port = $state<number | string>(587);
	let username = $state('');
	let password = $state('');
	let from_address = $state('');
	let from_name = $state('');
	let tls_mode = $state('starttls');
	let showPassword = $state(false);

	$effect(() => {
		const data = $query.data;
		if (data) {
			enabled = data.enabled;
			host = data.host ?? '';
			port = data.port ?? 587;
			username = data.username ?? '';
			password = '';
			from_address = data.from_address ?? '';
			from_name = data.from_name ?? '';
			tls_mode = data.tls_mode ?? 'starttls';
		}
	});

	const saveMutation = createMutation({
		mutationFn: () => {
			const data: SmtpConfigRequest = {
				enabled,
				host: host.trim() || undefined,
				port: port !== '' ? Number(port) : undefined,
				username: username.trim() || undefined,
				from_address: from_address.trim() || undefined,
				from_name: from_name.trim() || undefined,
				tls_mode: tls_mode || undefined
			};
			if (password.trim()) data.password = password.trim();
			return updateSmtpConfig(data);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'smtp-config'] });
		}
	});
</script>

<div class="card bg-base-100 shadow">
	<div class="card-body">
		<h3 class="card-title">SMTP Email Configuration</h3>

		{#if $query.isLoading}
			<div class="flex justify-center py-8"><span class="loading loading-spinner"></span></div>
		{:else if $query.isError}
			<div class="alert alert-error">Failed to load SMTP config.</div>
		{:else}
			<form
				onsubmit={(e) => {
					e.preventDefault();
					$saveMutation.mutate();
				}}
				class="mt-2 space-y-4"
			>
				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle toggle-primary" bind:checked={enabled} />
						<span class="label-text font-medium">Enable SMTP Email</span>
					</label>
				</div>

				<div class="divider"></div>

				<div class="grid grid-cols-1 gap-4 md:grid-cols-2">
					<div class="form-control">
						<label class="label" for="smtp-host"><span class="label-text">SMTP Host</span></label>
						<input
							id="smtp-host"
							type="text"
							class="input-bordered input"
							bind:value={host}
							placeholder="smtp.example.com"
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="smtp-port"><span class="label-text">Port</span></label>
						<input
							id="smtp-port"
							type="number"
							class="input-bordered input"
							bind:value={port}
							min="1"
							max="65535"
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="tls-mode"><span class="label-text">TLS Mode</span></label>
						<select
							id="tls-mode"
							class="select-bordered select"
							bind:value={tls_mode}
							disabled={!enabled}
						>
							<option value="none">None</option>
							<option value="starttls">STARTTLS</option>
							<option value="tls">TLS/SSL</option>
						</select>
					</div>

					<div class="form-control">
						<label class="label" for="smtp-username">
							<span class="label-text">Username</span>
						</label>
						<input
							id="smtp-username"
							type="text"
							class="input-bordered input"
							bind:value={username}
							disabled={!enabled}
							autocomplete="off"
						/>
					</div>

					<div class="form-control md:col-span-2">
						<label class="label" for="smtp-password">
							<span class="label-text">
								Password
								{#if $query.data?.password}
									<span class="ml-1 text-xs text-base-content/50"
										>(stored — leave blank to keep)</span
									>
								{/if}
							</span>
						</label>
						<div class="relative">
							<input
								id="smtp-password"
								type={showPassword ? 'text' : 'password'}
								class="input-bordered input w-full pr-10"
								bind:value={password}
								placeholder={$query.data?.password ? '••••••••' : 'Enter password'}
								disabled={!enabled}
								autocomplete="new-password"
							/>
							<button
								type="button"
								class="btn absolute top-1/2 right-2 -translate-y-1/2 btn-ghost btn-xs"
								onclick={() => (showPassword = !showPassword)}
								tabindex="-1"
							>
								{showPassword ? 'Hide' : 'Show'}
							</button>
						</div>
					</div>

					<div class="form-control">
						<label class="label" for="from-address">
							<span class="label-text">From Address</span>
						</label>
						<input
							id="from-address"
							type="email"
							class="input-bordered input"
							bind:value={from_address}
							placeholder="noreply@example.com"
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="from-name"><span class="label-text">From Name</span></label>
						<input
							id="from-name"
							type="text"
							class="input-bordered input"
							bind:value={from_name}
							placeholder="RustShare"
							disabled={!enabled}
						/>
					</div>
				</div>

				{#if $saveMutation.isError}
					<div class="alert text-sm alert-error">
						{$saveMutation.error instanceof Error
							? $saveMutation.error.message
							: 'Failed to save SMTP config'}
					</div>
				{/if}
				{#if $saveMutation.isSuccess}
					<div class="alert text-sm alert-success">SMTP configuration saved.</div>
				{/if}

				<div class="flex gap-3">
					<button type="submit" class="btn btn-primary" disabled={$saveMutation.isPending}>
						{$saveMutation.isPending ? 'Saving...' : 'Save Configuration'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</div>
