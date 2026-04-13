<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { beginOidcLogin, getAuthConfig, type AuthConfig } from '$lib/api/auth';
	import { authStore } from '$lib/stores/auth';

	let email = '';
	let password = '';
	let isLoading = false;
	let showError = false;
	let errorMessage = '';
	let isAuthConfigLoading = true;
	let authConfigError = '';
	let redirectTo = '/files';
	let authConfig: AuthConfig = {
		password_login_enabled: true,
		oidc_enabled: false,
		oidc_login_label: null,
		oidc_mobile_enabled: false
	};

	$: redirectTo = $page.url.searchParams.get('redirect_to') || '/files';

	$: hasAnyLoginMethod = authConfig.oidc_enabled || authConfig.password_login_enabled;

	$: if ($authStore.isAuthenticated && browser) {
		goto(redirectTo);
	}

	onMount(() => {
		void (async () => {
			try {
				authConfig = await getAuthConfig();
				authConfigError = '';
			} catch (error) {
				console.error('Failed to load auth configuration:', error);
				authConfigError =
					'RustShare could not confirm the active login mode. Password sign-in stays available as a fallback while the operator checks OIDC settings.';
			} finally {
				isAuthConfigLoading = false;
			}
		})();
	});

	async function handleLogin() {
		if (!email || !password) {
			showError = true;
			errorMessage = 'Please enter email and password';
			return;
		}

		isLoading = true;
		authStore.setLoading(true);

		try {
			await authStore.login(email, password);
			goto(redirectTo);
		} catch (error: any) {
			showError = true;
			errorMessage = error.message || 'Login failed. Please try again.';
		} finally {
			isLoading = false;
			authStore.setLoading(false);
		}
	}

	function handleSubmit(event: Event) {
		event.preventDefault();
		handleLogin();
	}

	function handleOidcLogin() {
		isLoading = true;
		beginOidcLogin(redirectTo);
	}
</script>

<svelte:head>
	<title>Sign in - RustShare</title>
</svelte:head>

<div class="relative min-h-screen overflow-hidden bg-base-100 px-4 py-8 lg:px-8 lg:py-10">
	<div class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(198,90,30,0.10),transparent_30%),radial-gradient(circle_at_bottom_right,rgba(123,74,46,0.08),transparent_24%)]"></div>

	<div class="relative mx-auto grid min-h-[calc(100vh-4rem)] max-w-6xl items-center gap-8 lg:grid-cols-[1.1fr_0.9fr]">
		<section class="order-2 lg:order-1">
			<div class="max-w-3xl">
				<p class="rs-kicker mb-6">Operational entrypoint</p>
				<h1 class="font-display max-w-[13ch] text-4xl leading-[0.96] text-base-content sm:text-5xl xl:text-6xl">
					Controlled file access starts with a calm login.
				</h1>
				<p class="mt-6 max-w-2xl text-base leading-7 text-base-content/68 xl:text-lg">
					RustShare is built for teams that need files, identity, and hosting under their own
					control. This screen is for system access, not product marketing.
				</p>
			</div>

			<div class="mt-8 space-y-3">
				<div class="flex items-start gap-4 rounded-xl border border-base-300/70 bg-base-100/60 px-4 py-3.5">
					<div class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500">
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="h-4 w-4">
							<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
							<polyline points="10 17 15 12 10 7"/>
						</svg>
					</div>
					<div>
						<p class="text-sm font-semibold text-base-content">OIDC-first identity</p>
						<p class="text-sm text-base-content/60">Connects to your existing source of truth.</p>
					</div>
				</div>
				<div class="flex items-start gap-4 rounded-xl border border-base-300/70 bg-base-100/60 px-4 py-3.5">
					<div class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500">
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="h-4 w-4">
							<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>
							<path d="M7 11V7a5 5 0 0 1 10 0v4"/>
						</svg>
					</div>
					<div>
						<p class="text-sm font-semibold text-base-content">Self-hosted control</p>
						<p class="text-sm text-base-content/60">No licensing traps or vendor lock-in.</p>
					</div>
				</div>
				<div class="flex items-start gap-4 rounded-xl border border-base-300/70 bg-base-100/60 px-4 py-3.5">
					<div class="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-500">
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="h-4 w-4">
							<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
							<path d="m9 12 2 2 4-4"/>
						</svg>
					</div>
					<div>
						<p class="text-sm font-semibold text-base-content">Clear recovery paths</p>
						<p class="text-sm text-base-content/60">Explicit fallback when identity providers need attention.</p>
					</div>
				</div>
			</div>
		</section>

		<section class="order-1 mx-auto w-full max-w-md rounded-[1.75rem] border border-base-300/80 bg-base-100/94 p-6 shadow-panel backdrop-blur-xl sm:p-8 lg:order-2">
			<div class="mb-8">
				<div class="mb-5 flex h-16 w-16 items-center justify-center rounded-[1.35rem] bg-gradient-to-br from-brand-500 to-brand-600 shadow-lg shadow-brand-500/20">
					<svg class="h-10 w-10 text-white" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
						<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor" />
						<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-300" />
						<circle cx="24" cy="21" r="5" fill="#121315" />
						<circle cx="24" cy="21" r="3" fill="currentColor" />
						<rect x="22.5" y="19.5" width="3" height="3" fill="#121315" />
					</svg>
				</div>
				<h2 class="font-display text-4xl leading-none text-base-content">RustShare</h2>
				<p class="mt-3 text-sm leading-6 text-base-content/65">
					Sign in to the file workspace your organization controls.
				</p>
			</div>

			<div class="mb-6 rounded-[1.1rem] border border-base-300/70 bg-base-200/55 px-4 py-3">
				<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/42">
					Trust statement
				</p>
				<p class="mt-2 font-data text-sm leading-6 text-base-content/74">
					Use organization SSO when it is configured. Password sign-in stays visible only as
					an explicit fallback.
				</p>
			</div>

			{#if isAuthConfigLoading}
				<div class="flex items-center justify-center rounded-xl border border-base-300/80 bg-base-200/35 px-4 py-6" aria-live="polite">
					<span class="loading loading-spinner loading-md"></span>
				</div>
			{:else}
				{#if authConfigError}
					<div class="alert alert-warning mb-4 text-sm" role="alert">
						<span>{authConfigError}</span>
					</div>
				{/if}

				{#if authConfig.oidc_enabled}
					<button
						type="button"
						class="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-base-300/80 bg-base-100 px-4 py-3 text-sm font-semibold text-base-content transition-colors hover:border-brand-500/25 hover:bg-base-200 disabled:opacity-50"
						on:click={handleOidcLogin}
						disabled={isLoading}
					>
						<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="h-5 w-5" aria-hidden="true">
							<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" />
							<polyline points="10,17 15,12 10,7" />
							<line x1="15" x2="3" y1="12" y2="12" />
						</svg>
						{authConfig.oidc_login_label || 'Continue with SSO'}
					</button>
				{/if}

				{#if authConfig.oidc_enabled && authConfig.password_login_enabled}
					<div class="relative my-6">
						<div class="absolute inset-0 flex items-center">
							<div class="w-full border-t border-base-300/80"></div>
						</div>
						<div class="relative flex justify-center text-sm">
							<span class="bg-base-100 px-3 font-data text-xs font-semibold uppercase tracking-[0.14em] text-base-content/42">
								Password fallback
							</span>
						</div>
					</div>
				{/if}

				{#if authConfig.password_login_enabled}
					<form on:submit={handleSubmit} class="space-y-4">
						<div>
							<label for="email" class="mb-1.5 block text-sm font-semibold text-base-content">Email</label>
							<input
								id="email"
								type="email"
								placeholder="you@example.com"
								class="rs-field"
								bind:value={email}
								disabled={isLoading}
								autocomplete="username"
							/>
						</div>

						<div>
							<label for="password" class="mb-1.5 block text-sm font-semibold text-base-content">Password</label>
							<input
								id="password"
								type="password"
								placeholder="••••••••"
								class="rs-field"
								bind:value={password}
								disabled={isLoading}
								autocomplete="current-password"
							/>
						</div>

						<button
							type="submit"
							class="inline-flex min-h-11 w-full items-center justify-center gap-2 rounded-xl bg-brand-500 px-4 py-3 text-sm font-semibold text-white shadow-lg shadow-brand-500/20 transition-colors hover:bg-brand-600 disabled:opacity-50"
							disabled={isLoading}
						>
							{#if isLoading}
								<span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-white/30 border-t-white"></span>
								Signing in...
							{:else}
								Sign in with password
							{/if}
						</button>
					</form>
				{/if}

				{#if !hasAnyLoginMethod}
					<div class="rounded-xl border border-warning/25 bg-warning/10 p-4 text-sm leading-6 text-warning" role="alert">
						No login method is enabled for this deployment. Save OIDC settings in the admin
						control plane or re-enable password login before inviting users.
					</div>
				{/if}
			{/if}

			<div class="mt-6 rounded-[1.1rem] border border-base-300/70 bg-base-200/55 px-4 py-3">
				<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/42">
					Operator note
				</p>
				<p class="mt-2 font-data text-sm leading-6 text-base-content/72">
					If SSO is missing here, the runtime config is probably incomplete rather than
					ignored. Check the admin OIDC page first.
				</p>
			</div>
		</section>
	</div>
</div>
