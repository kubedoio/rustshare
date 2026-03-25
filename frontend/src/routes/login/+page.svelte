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

<div class="min-h-screen bg-base-100 flex items-center justify-center p-4">
	<div class="w-full max-w-md">
		<!-- Logo -->
		<div class="flex flex-col items-center mb-8">
			<div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-brand-500 to-brand-600 flex items-center justify-center mb-4 shadow-lg shadow-brand-500/20">
				<svg class="w-10 h-10 text-white" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
					<rect x="2" y="6" width="28" height="20" rx="3" fill="currentColor"/>
					<rect x="2" y="9" width="28" height="4" fill="currentColor" class="text-brand-300"/>
					<circle cx="24" cy="21" r="5" fill="#0f1115"/>
					<circle cx="24" cy="21" r="3" fill="currentColor"/>
					<rect x="22.5" y="19.5" width="3" height="3" fill="#0f1115"/>
				</svg>
			</div>
			<h1 class="text-2xl font-bold text-base-content">RustShare</h1>
			<p class="text-sm text-base-content/60 mt-1">Sign in to your account</p>
		</div>

		<!-- Login Card -->
		<div class="bg-base-200 rounded-2xl border border-base-300 p-6">
			{#if authConfig.oidc_enabled}
				<button
					type="button"
					class="w-full flex items-center justify-center gap-2 px-4 py-3 bg-base-100 hover:bg-base-300 border border-base-300 rounded-xl text-sm font-medium text-base-content transition-colors"
					on:click={handleOidcLogin}
					disabled={isLoading}
				>
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5">
						<path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
						<polyline points="10,17 15,12 10,7"/>
						<line x1="15" x2="3" y1="12" y2="12"/>
					</svg>
					{authConfig.oidc_login_label || 'Continue with SSO'}
				</button>
			{/if}

			{#if authConfig.oidc_enabled && authConfig.password_login_enabled}
				<div class="relative my-6">
					<div class="absolute inset-0 flex items-center">
						<div class="w-full border-t border-base-300"></div>
					</div>
					<div class="relative flex justify-center text-sm">
						<span class="px-2 bg-base-200 text-base-content/50">or</span>
					</div>
				</div>
			{/if}

			{#if authConfig.password_login_enabled}
				<form on:submit={handleSubmit} class="space-y-4">
					<div>
						<label for="email" class="block text-sm font-medium text-base-content mb-1.5">Email</label>
						<input
							id="email"
							type="email"
							placeholder="you@example.com"
							class="w-full px-4 py-3 bg-base-100 border border-base-300 rounded-xl text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50 transition-colors"
							bind:value={email}
							disabled={isLoading}
						/>
					</div>

					<div>
						<label for="password" class="block text-sm font-medium text-base-content mb-1.5">Password</label>
						<input
							id="password"
							type="password"
							placeholder="••••••••"
							class="w-full px-4 py-3 bg-base-100 border border-base-300 rounded-xl text-sm text-base-content placeholder:text-base-content/40 focus:outline-none focus:border-brand-500/50 transition-colors"
							bind:value={password}
							disabled={isLoading}
						/>
					</div>

					<button
						type="submit"
						class="w-full flex items-center justify-center gap-2 px-4 py-3 bg-brand-500 hover:bg-brand-600 text-white rounded-xl text-sm font-medium transition-colors shadow-lg shadow-brand-500/20 disabled:opacity-50"
						disabled={isLoading}
					>
						{#if isLoading}
							<span class="inline-block w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
							Signing in...
						{:else}
							Sign in
						{/if}
					</button>
				</form>
			{:else if !authConfig.oidc_enabled}
				<div class="p-4 bg-warning/10 border border-warning/20 rounded-xl text-center">
					<p class="text-sm text-warning">No login method is enabled for this deployment.</p>
				</div>
			{/if}
		</div>

		<!-- Footer -->
		<p class="text-center text-xs text-base-content/40 mt-6">
			RustShare - Secure File Sharing
		</p>
	</div>
</div>

{#if showError}
	<Toast message={errorMessage} type="error" onClose={() => (showError = false)} />
{/if}
