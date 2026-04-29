<script lang="ts">
	import { createMutation, createQuery, useQueryClient } from '$lib/query-compat';
	import {
		getOidcConfig,
		testOidcConfig,
		updateOidcConfig,
		type OidcConfigRequest
	} from '$lib/api/admin';

	const queryClient = useQueryClient();

	const query = createQuery({
		queryKey: ['admin', 'oidc-config'],
		queryFn: getOidcConfig
	});

	let enabled = false;
	let provider_name = '';
	let client_id = '';
	let client_secret = '';
	let issuer_url = '';
	let redirect_url = '';
	let login_label = '';
	let scopes_str = '';
	let auto_provision_users = false;
	let showSecret = false;

	let testResult: { success: boolean; message?: string } | null = null;

	$: storedSecretExists = Boolean($query.data?.client_secret);
	$: isConnectionReady =
		Boolean(issuer_url.trim()) &&
		Boolean(client_id.trim()) &&
		Boolean(redirect_url.trim()) &&
		(storedSecretExists || Boolean(client_secret.trim()));
	$: hasSaveSuccess = $saveMutation.isSuccess;
	$: setupSteps = [
		{
			key: 'identity',
			label: 'Identity provider details',
			done: Boolean(enabled && provider_name.trim() && issuer_url.trim())
		},
		{
			key: 'application',
			label: 'Application credentials',
			done: Boolean(enabled && client_id.trim() && (storedSecretExists || client_secret.trim()))
		},
		{
			key: 'runtime',
			label: 'RustShare runtime settings',
			done: Boolean(enabled && redirect_url.trim() && login_label.trim())
		},
		{
			key: 'verify',
			label: 'Verify discovery and save',
			done: Boolean(testResult?.success || hasSaveSuccess)
		}
	];

	$: if ($query.data) {
		enabled = $query.data.enabled;
		provider_name = $query.data.provider_name ?? '';
		client_id = $query.data.client_id ?? '';
		client_secret = '';
		issuer_url = $query.data.issuer_url ?? '';
		redirect_url = $query.data.redirect_url ?? '';
		login_label = $query.data.login_label ?? 'Continue with SSO';
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
				redirect_url: redirect_url.trim() || undefined,
				login_label: login_label.trim() || undefined,
				scopes: scopes_str.trim() ? scopes_str.trim().split(/\s+/) : undefined,
				auto_provision_users
			};
			if (client_secret.trim()) data.client_secret = client_secret.trim();
			return updateOidcConfig(data);
		},
		onSuccess: () => {
			testResult = null;
			queryClient.invalidateQueries({ queryKey: ['admin', 'oidc-config'] });
		}
	});

	const testMutation = createMutation({
		mutationFn: () => testOidcConfig(),
		onSuccess: (res) => {
			testResult = res;
		},
		onError: (err) => {
			testResult = {
				success: false,
				message: err instanceof Error ? err.message : 'Could not reach the discovery endpoint'
			};
		}
	});
</script>

<div class="card border border-base-300/80 bg-base-100 shadow">
	<div class="card-body gap-6">
		<div class="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
			<div class="max-w-2xl">
				<h3 class="card-title text-2xl">OIDC / SSO Configuration</h3>
				<p class="mt-2 text-sm leading-6 text-base-content/70">
					Bootstrap can seed these fields once from environment variables, but the saved admin
					settings are the runtime source of truth after that. Treat this page as the operator
					control plane for SSO.
				</p>
			</div>
			<div
				class="rounded-2xl border border-base-300/80 bg-base-200/50 px-4 py-3 text-sm text-base-content/70"
			>
				<p class="font-semibold text-base-content">Pilot setup path</p>
				<p class="mt-1">
					Fill provider details, save once, test discovery, then verify the login screen.
				</p>
			</div>
		</div>

		{#if $query.isLoading}
			<div class="flex justify-center py-10" aria-live="polite">
				<span class="loading loading-md loading-spinner"></span>
			</div>
		{:else if $query.isError}
			<div class="alert alert-error" role="alert">
				<span>Failed to load OIDC settings. The admin control plane is unavailable right now.</span>
			</div>
		{:else}
			<form on:submit|preventDefault={() => $saveMutation.mutate()} class="space-y-6">
				<section class="rounded-[1.4rem] border border-base-300/80 bg-base-200/35 p-5">
					<div class="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
						<div class="max-w-2xl">
							<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
								Setup Status
							</p>
							<p class="mt-2 text-sm leading-6 text-base-content/70">
								This keeps the existing settings page, but shows the same sequence an operator will
								actually follow during a pilot.
							</p>
						</div>
						<label
							class="label cursor-pointer justify-start gap-3 rounded-xl border border-base-300/80 bg-base-100 px-4 py-3"
						>
							<input type="checkbox" class="toggle toggle-primary" bind:checked={enabled} />
							<span class="label-text font-medium">Enable OIDC / SSO</span>
						</label>
					</div>

					<div class="mt-5 grid gap-3 md:grid-cols-2 xl:grid-cols-4">
						{#each setupSteps as step}
							<div class="rounded-xl border border-base-300/80 bg-base-100 px-4 py-3">
								<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
									{step.key}
								</p>
								<p class="mt-2 text-sm font-medium text-base-content">{step.label}</p>
								<p
									class="mt-2 text-xs font-semibold {step.done
										? 'text-success'
										: 'text-base-content/45'}"
								>
									{step.done ? 'Ready' : 'Pending'}
								</p>
							</div>
						{/each}
					</div>
				</section>

				<section class="grid gap-4 lg:grid-cols-2">
					<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Step 1
						</p>
						<h4 class="mt-2 text-lg font-semibold text-base-content">Identity provider details</h4>
						<p class="mt-2 text-sm leading-6 text-base-content/68">
							Start with the issuer your organization already trusts. If discovery works here, the
							login screen can use the same provider without extra environment wiring.
						</p>

						<div class="mt-5 space-y-4">
							<div class="form-control">
								<label class="label" for="provider-name">
									<span class="label-text">Provider name</span>
								</label>
								<input
									id="provider-name"
									type="text"
									class="input-bordered input"
									bind:value={provider_name}
									placeholder="Keycloak, Microsoft Entra, Okta"
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
									class="input-bordered input"
									bind:value={issuer_url}
									placeholder="https://accounts.example.com/realms/team"
									aria-describedby="issuer-help"
									disabled={!enabled}
								/>
								<p id="issuer-help" class="mt-2 text-xs leading-5 text-base-content/55">
									RustShare uses discovery from this issuer to find the authorize and token
									endpoints.
								</p>
							</div>
						</div>
					</div>

					<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Step 2
						</p>
						<h4 class="mt-2 text-lg font-semibold text-base-content">Application credentials</h4>
						<p class="mt-2 text-sm leading-6 text-base-content/68">
							These values stay in the database so the login path and admin UI read the same
							configuration at runtime.
						</p>

						<div class="mt-5 space-y-4">
							<div class="form-control">
								<label class="label" for="client-id">
									<span class="label-text">Client ID</span>
								</label>
								<input
									id="client-id"
									type="text"
									class="input-bordered input"
									bind:value={client_id}
									disabled={!enabled}
								/>
							</div>

							<div class="form-control">
								<label class="label" for="client-secret">
									<span class="label-text">
										Client secret
										{#if storedSecretExists}
											<span class="ml-1 text-xs text-base-content/50"
												>(stored - leave blank to keep)</span
											>
										{/if}
									</span>
								</label>
								<div class="relative">
									<input
										id="client-secret"
										type={showSecret ? 'text' : 'password'}
										class="input-bordered input w-full pr-14"
										bind:value={client_secret}
										placeholder={storedSecretExists ? '••••••••' : 'Enter secret'}
										disabled={!enabled}
									/>
									<button
										type="button"
										class="btn absolute top-1/2 right-2 -translate-y-1/2 btn-ghost btn-xs"
										on:click={() => (showSecret = !showSecret)}
									>
										{showSecret ? 'Hide' : 'Show'}
									</button>
								</div>
							</div>

							<div class="form-control">
								<label class="label" for="scopes">
									<span class="label-text">Scopes</span>
								</label>
								<input
									id="scopes"
									type="text"
									class="input-bordered input"
									bind:value={scopes_str}
									placeholder="openid email profile"
									disabled={!enabled}
								/>
							</div>
						</div>
					</div>
				</section>

				<section class="grid gap-4 lg:grid-cols-[1.15fr_0.85fr]">
					<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Step 3
						</p>
						<h4 class="mt-2 text-lg font-semibold text-base-content">RustShare runtime settings</h4>
						<p class="mt-2 text-sm leading-6 text-base-content/68">
							These are the values the live login screen uses. Save them here once so the operator
							view and runtime behavior stay aligned.
						</p>

						<div class="mt-5 grid gap-4 md:grid-cols-2">
							<div class="form-control md:col-span-2">
								<label class="label" for="redirect-url">
									<span class="label-text">Redirect URL</span>
								</label>
								<input
									id="redirect-url"
									type="url"
									class="input-bordered input"
									bind:value={redirect_url}
									placeholder="https://files.example.edu/api/v1/auth/oidc/callback"
									aria-describedby="redirect-help"
									disabled={!enabled}
								/>
								<p id="redirect-help" class="mt-2 text-xs leading-5 text-base-content/55">
									Use the exact callback URL registered with your identity provider.
								</p>
							</div>

							<div class="form-control md:col-span-2">
								<label class="label" for="login-label">
									<span class="label-text">Login button label</span>
								</label>
								<input
									id="login-label"
									type="text"
									class="input-bordered input"
									bind:value={login_label}
									placeholder="Continue with SSO"
									disabled={!enabled}
								/>
							</div>
						</div>
					</div>

					<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100 p-5">
						<p class="text-xs font-semibold tracking-[0.14em] text-base-content/45 uppercase">
							Step 4
						</p>
						<h4 class="mt-2 text-lg font-semibold text-base-content">Pilot guardrails</h4>
						<p class="mt-2 text-sm leading-6 text-base-content/68">
							Keep provisioning intentional for the first pilot and verify discovery before asking
							anyone else to use the login page.
						</p>

						<label
							class="label mt-5 cursor-pointer justify-start gap-3 rounded-xl border border-base-300/80 bg-base-200/40 px-4 py-3"
						>
							<input
								type="checkbox"
								class="checkbox"
								bind:checked={auto_provision_users}
								disabled={!enabled}
							/>
							<span class="label-text leading-6"> Auto-provision new users on first login </span>
						</label>

						<div
							class="mt-5 rounded-xl border border-base-300/80 bg-base-200/40 px-4 py-3 text-sm leading-6 text-base-content/68"
						>
							<p class="font-semibold text-base-content">What the operator sees next</p>
							<p class="mt-2">
								1. Save the config here.
								<br />
								2. Test discovery.
								<br />
								3. Open the login page and confirm the SSO button appears.
							</p>
						</div>
					</div>
				</section>

				{#if $saveMutation.isError}
					<div class="alert text-sm alert-error" role="alert">
						{$saveMutation.error instanceof Error
							? $saveMutation.error.message
							: 'Failed to save OIDC configuration'}
					</div>
				{/if}

				{#if hasSaveSuccess}
					<div class="alert text-sm alert-success" role="status">
						Runtime OIDC settings saved. The login page will read these values from the database.
					</div>
				{/if}

				{#if testResult}
					<div
						class="alert text-sm"
						class:alert-success={testResult.success}
						class:alert-error={!testResult.success}
						role="status"
					>
						{testResult.message ??
							(testResult.success
								? 'Discovery document fetched successfully'
								: 'Connection failed')}
					</div>
				{/if}

				<div class="flex flex-col gap-3 sm:flex-row">
					<button type="submit" class="btn btn-primary" disabled={$saveMutation.isPending}>
						{$saveMutation.isPending ? 'Saving...' : 'Save runtime configuration'}
					</button>
					<button
						type="button"
						class="btn btn-outline"
						disabled={$testMutation.isPending || !enabled || !isConnectionReady}
						on:click={() => $testMutation.mutate()}
					>
						{$testMutation.isPending ? 'Testing...' : 'Test discovery'}
					</button>
				</div>
			</form>
		{/if}
	</div>
</div>
