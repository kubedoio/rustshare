<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { getOidcConfig, updateOidcConfig, testOidcConfig, type OidcConfigRequest } from '$lib/api/admin';

	const query = createQuery({
		queryKey: ['admin', 'oidc-config'],
		queryFn: getOidcConfig
	});

	let enabled = false;
	let provider_name = '';
	let client_id = '';
	let client_secret = '';
	let issuer_url = '';
	let scopes_str = '';
	let auto_provision_users = false;
	let showSecret = false;

	let testResult: { success: boolean; message?: string } | null = null;

	$: if ($query.data) {
		enabled = $query.data.enabled;
		provider_name = $query.data.provider_name ?? '';
		client_id = $query.data.client_id ?? '';
		client_secret = '';
		issuer_url = $query.data.issuer_url ?? '';
		scopes_str = ($query.data.scopes ?? []).join(' ');
		auto_provision_users = $query.data.auto_provision_users;
	}

	const saveMutation = createMutation({
		mutationFn: () => {
			const data: OidcConfigRequest = {
				enabled,
				provider_name: provider_name.trim() || undefined,
				client_id: client_id.trim() || undefined,
				issuer_url: issuer_url.trim() || undefined,
				scopes: scopes_str.trim() ? scopes_str.trim().split(/\s+/) : undefined,
				auto_provision_users
			};
			if (client_secret.trim()) data.client_secret = client_secret.trim();
			return updateOidcConfig(data);
		}
	});

	const testMutation = createMutation({
		mutationFn: () => testOidcConfig(),
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
		<h3 class="card-title">OIDC / SSO Configuration</h3>

		{#if $query.isLoading}
			<div class="flex justify-center py-8"><span class="loading loading-spinner"></span></div>
		{:else if $query.isError}
			<div class="alert alert-error">Failed to load OIDC config.</div>
		{:else}
			<form
				on:submit|preventDefault={() => $saveMutation.mutate()}
				class="space-y-4 mt-2"
			>
				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input type="checkbox" class="toggle toggle-primary" bind:checked={enabled} />
						<span class="label-text font-medium">Enable OIDC/SSO</span>
					</label>
				</div>

				<div class="divider"></div>

				<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
					<div class="form-control">
						<label class="label" for="provider-name">
							<span class="label-text">Provider Name</span>
						</label>
						<input
							id="provider-name"
							type="text"
							class="input input-bordered"
							bind:value={provider_name}
							placeholder="e.g. Google, Okta, Keycloak"
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="issuer-url">
							<span class="label-text">Issuer URL</span>
						</label>
						<input
							id="issuer-url"
							type="url"
							class="input input-bordered"
							bind:value={issuer_url}
							placeholder="https://accounts.example.com"
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="client-id">
							<span class="label-text">Client ID</span>
						</label>
						<input
							id="client-id"
							type="text"
							class="input input-bordered"
							bind:value={client_id}
							disabled={!enabled}
						/>
					</div>

					<div class="form-control">
						<label class="label" for="client-secret">
							<span class="label-text">
								Client Secret
								{#if $query.data?.client_secret}
									<span class="text-xs text-base-content/50 ml-1">(stored — leave blank to keep)</span>
								{/if}
							</span>
						</label>
						<div class="relative">
							<input
								id="client-secret"
								type={showSecret ? 'text' : 'password'}
								class="input input-bordered w-full pr-10"
								bind:value={client_secret}
								placeholder={$query.data?.client_secret ? '••••••••' : 'Enter secret'}
								disabled={!enabled}
							/>
							<button
								type="button"
								class="absolute right-2 top-1/2 -translate-y-1/2 btn btn-ghost btn-xs"
								on:click={() => (showSecret = !showSecret)}
								tabindex="-1"
							>
								{showSecret ? 'Hide' : 'Show'}
							</button>
						</div>
					</div>

					<div class="form-control md:col-span-2">
						<label class="label" for="scopes">
							<span class="label-text">Scopes (space-separated)</span>
						</label>
						<input
							id="scopes"
							type="text"
							class="input input-bordered"
							bind:value={scopes_str}
							placeholder="openid email profile"
							disabled={!enabled}
						/>
					</div>
				</div>

				<div class="form-control">
					<label class="label cursor-pointer justify-start gap-3">
						<input
							type="checkbox"
							class="checkbox"
							bind:checked={auto_provision_users}
							disabled={!enabled}
						/>
						<span class="label-text">Auto-provision new users on first login</span>
					</label>
				</div>

				{#if $saveMutation.isError}
					<div class="alert alert-error text-sm">
						{$saveMutation.error instanceof Error
							? $saveMutation.error.message
							: 'Failed to save config'}
					</div>
				{/if}
				{#if $saveMutation.isSuccess}
					<div class="alert alert-success text-sm">Configuration saved.</div>
				{/if}

				{#if testResult !== null}
					<div class="alert" class:alert-success={testResult.success} class:alert-error={!testResult.success}>
						{testResult.message ?? (testResult.success ? 'Connection successful' : 'Connection failed')}
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
						{$testMutation.isPending ? 'Testing...' : 'Test Connection'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</div>
