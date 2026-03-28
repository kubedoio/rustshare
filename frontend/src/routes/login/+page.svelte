<script lang="ts">
	import { onMount } from 'svelte';
	import { authStore } from '$lib/stores/auth';
	import { beginOidcLogin, getAuthConfig, type AuthConfig } from '$lib/api/auth';
	import { goto } from '$app/navigation';
	import Toast from '$lib/components/common/Toast.svelte';

	let email = '';
	let password = '';
	let isLoading = false;
	let errorMessage = '';
	let showError = false;
	let authConfig: AuthConfig = {
		password_login_enabled: true,
		oidc_enabled: false,
		oidc_login_label: null,
		oidc_mobile_enabled: false
	};

	onMount(async () => {
		try {
			authConfig = await getAuthConfig();
		} catch (error) {
			console.error('Failed to load auth configuration:', error);
		}
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
			goto('/files');
		} catch (error: any) {
			showError = true;
			errorMessage = error.message || 'Login failed. Please try again.';
		} finally {
			isLoading = false;
			authStore.setLoading(false);
		}
	}

	function handleSubmit(e: Event) {
		e.preventDefault();
		handleLogin();
	}

	function handleOidcLogin() {
		isLoading = true;
		beginOidcLogin('/files');
	}
</script>

<svelte:head>
	<title>Sign in - RustShare</title>
</svelte:head>

<div class="relative min-h-screen overflow-hidden bg-base-100 px-4 py-8 lg:px-8 lg:py-10">
	<div class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(198,90,30,0.12),transparent_28%),radial-gradient(circle_at_bottom_right,rgba(123,74,46,0.10),transparent_24%)]"></div>
	<div class="relative mx-auto grid min-h-[calc(100vh-4rem)] max-w-6xl items-center gap-8 lg:grid-cols-[1.05fr_0.95fr]">
		<section class="hidden lg:block">
			<div class="rs-kicker mb-6">Private-cloud file operations</div>
			<h1 class="font-display max-w-[11ch] text-5xl leading-[0.95] text-base-content xl:text-6xl">
				Governed files for teams that need control.
			</h1>
			<p class="mt-6 max-w-2xl text-base leading-7 text-base-content/68 xl:text-lg">
				RustShare is built for technical organizations that care about clear permissions, audit trails, sovereign deployment, and calm daily usability.
			</p>
			<div class="mt-10 grid max-w-2xl gap-4 sm:grid-cols-3">
				<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100/80 p-5 shadow-panel">
					<p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Deployment</p>
					<p class="mt-3 font-data text-sm font-medium text-base-content">Self-hosted or private cloud, without permission ambiguity.</p>
				</div>
				<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100/80 p-5 shadow-panel">
					<p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Governance</p>
					<p class="mt-3 font-data text-sm font-medium text-base-content">Share expiry, activity history, and operational visibility by default.</p>
				</div>
				<div class="rounded-[1.4rem] border border-base-300/80 bg-base-100/80 p-5 shadow-panel">
					<p class="text-xs font-semibold uppercase tracking-[0.16em] text-base-content/42">Identity</p>
					<p class="mt-3 font-data text-sm font-medium text-base-content">OIDC-first access for teams that already have a source of truth.</p>
				</div>
			</div>
		</section>

		<section class="mx-auto w-full max-w-md rounded-[1.75rem] border border-base-300/80 bg-base-100/92 p-6 shadow-panel backdrop-blur-xl sm:p-8">
			<div class="mb-8 flex flex-col items-center text-center">
				<div class="mb-5 flex h-20 w-20 items-center justify-center rounded-[1.6rem] bg-gradient-to-br from-brand-500 to-brand-600 shadow-lg shadow-brand-500/20">
					<svg class="h-12 w-12 text-white" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor"/>
						<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-300"/>
						<circle cx="24" cy="21" r="5" fill="#121315"/>
						<circle cx="24" cy="21" r="3" fill="currentColor"/>
						<rect x="22.5" y="19.5" width="3" height="3" fill="#121315"/>
					</svg>
				</div>
				<h2 class="font-display text-4xl leading-none text-base-content">RustShare</h2>
				<p class="mt-3 max-w-sm text-sm leading-6 text-base-content/62">
					Sign in to your workspace and pick up where your files, links, and devices left off.
				</p>
			</div>

			{#if authConfig.oidc_enabled}
				<button
					type="button"
					class="inline-flex w-full items-center justify-center gap-2 rounded-xl border border-base-300/80 bg-base-100 px-4 py-3 text-sm font-semibold text-base-content transition-colors hover:border-brand-500/20 hover:bg-base-200"
					on:click={handleOidcLogin}
					disabled={isLoading}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
						<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
						<polyline points="10,17 15,12 10,7"/>
						<line x1="15" x2="3" y1="12" y2="12"/>
					</svg>
					{authConfig.oidc_login_label || 'Continue with OIDC'}
				</button>
			{/if}

			{#if authConfig.oidc_enabled && authConfig.password_login_enabled}
				<div class="relative my-6">
					<div class="absolute inset-0 flex items-center">
						<div class="w-full border-t border-base-300/80"></div>
					</div>
					<div class="relative flex justify-center text-sm">
						<span class="bg-base-100 px-3 font-data text-xs font-semibold uppercase tracking-[0.14em] text-base-content/42">or sign in with password</span>
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
						/>
					</div>

					<button
						type="submit"
						class="inline-flex w-full items-center justify-center gap-2 rounded-xl bg-brand-500 px-4 py-3 text-sm font-semibold text-white shadow-lg shadow-brand-500/20 transition-colors hover:bg-brand-600 disabled:opacity-50"
						disabled={isLoading}
					>
						{#if isLoading}
							<span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-white/30 border-t-white"></span>
							Signing in...
						{:else}
							Sign in to RustShare
						{/if}
					</button>
				</form>
			{:else if !authConfig.oidc_enabled}
				<div class="rounded-xl border border-warning/20 bg-warning/10 p-4 text-center">
					<p class="text-sm text-warning">No login method is enabled for this deployment.</p>
				</div>
			{/if}

			<div class="mt-6 rounded-[1.1rem] border border-base-300/70 bg-base-200/60 px-4 py-3">
				<p class="text-xs font-semibold uppercase tracking-[0.14em] text-base-content/42">Operational note</p>
				<p class="mt-2 font-data text-sm text-base-content/72">
					Use this product when the file system is part of your control plane, not just a dumping ground.
				</p>
			</div>
		</section>
	</div>
</div>

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
