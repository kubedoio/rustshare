<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '$lib/query-compat';
	import { getSmtpConfig, updateSmtpConfig, testSmtpConfig, type SmtpConfigRequest } from '$lib/api/admin';

	const queryClient = useQueryClient();

	const query = createQuery({
		queryKey: ['admin', 'smtp-config'],
		queryFn: getSmtpConfig
	});

	let enabled = false;
	let host = '';
	let port: number | string = 587;
	let username = '';
	let password = '';
	let from_address = '';
	let from_name = '';
	let tls_mode = 'starttls';
	let showPassword = false;

	let testResult: { success: boolean; message?: string } | null = null;

	$: if ($query.data) {
		enabled = $query.data.enabled;
		host = $query.data.host ?? '';
		port = $query.data.port ?? 587;
		username = $query.data.username ?? '';
		password = '';
		from_address = $query.data.from_address ?? '';
		from_name = $query.data.from_name ?? '';
		tls_mode = $query.data.tls_mode ?? 'starttls';
	}

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

	const testMutation = createMutation({
		mutationFn: () => testSmtpConfig(),
		onSuccess: (res) => {
			testResult = res;
		},
		onError: (err) => {
			testResult = { success: false, message: err instanceof Error ? err.message : 'Test failed' };
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
			<form on:submit|preventDefault={() => $saveMutation.mutate()} class="space-y-4 mt-2">
				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle toggle-primary" bind:checked={enabled} />
						<span class="label-text font-medium">Enable SMTP Email</span>
					</label>
				</div>

				<div class="divider"></div>

				<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
					<div class="form-control">
						<label class="label" for="smtp-host"><span class="label-text">SMTP Host</span></label>
						<input
							id="smtp-host"
							type="text"
							class="input input-bordered"
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
							class="input input-bordered"
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
							class="select select-bordered"
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
							class="input input-bordered"
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
									<span class="text-xs text-base-content/50 ml-1">(stored — leave blank to keep)</span>
								{/if}
							</span>
						</label>
						<div class="relative">
							<input
								id="smtp-password"
								type={showPassword ? 'text' : 'password'}
								class="input input-bordered w-full pr-10"
								bind:value={password}
								placeholder={$query.data?.password ? '••••••••' : 'Enter password'}
								disabled={!enabled}
								autocomplete="new-password"
							/>
							<button
								type="button"
								class="absolute right-2 top-1/2 -translate-y-1/2 btn btn-ghost btn-xs"
								on:click={() => (showPassword = !showPassword)}
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
							class="input input-bordered"
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
							class="input input-bordered"
							bind:value={from_name}
							placeholder="RustShare"
							disabled={!enabled}
						/>
					</div>
				</div>

				{#if $saveMutation.isError}
					<div class="alert alert-error text-sm">
						{$saveMutation.error instanceof Error
							? $saveMutation.error.message
							: 'Failed to save SMTP config'}
					</div>
				{/if}
				{#if $saveMutation.isSuccess}
					<div class="alert alert-success text-sm">SMTP configuration saved.</div>
				{/if}

				{#if testResult !== null}
					<div
						class="alert text-sm"
						class:alert-success={testResult.success}
						class:alert-error={!testResult.success}
					>
						{testResult.message ?? (testResult.success ? 'Test email sent successfully' : 'Test failed')}
					</div>
				{/if}

				<div class="flex gap-3">
					<button type="submit" class="btn btn-primary" disabled={$saveMutation.isPending}>
						{$saveMutation.isPending ? 'Saving...' : 'Save Configuration'}
					</button>
					<button
						type="button"
						class="btn btn-outline"
						disabled={$testMutation.isPending || !enabled}
						on:click={() => $testMutation.mutate()}
					>
						{$testMutation.isPending ? 'Sending...' : 'Send Test Email'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</div>
