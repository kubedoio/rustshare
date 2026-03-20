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
    oidc_login_label: null
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
  <title>Login - RustShare</title>
</svelte:head>

<div class="min-h-screen flex items-center justify-center bg-base-200">
  <div class="card w-96 bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-2xl justify-center mb-4">RustShare</h2>

      {#if authConfig.oidc_enabled}
        <button
          type="button"
          class="btn btn-outline w-full"
          on:click={handleOidcLogin}
          disabled={isLoading}
        >
          {authConfig.oidc_login_label || 'Continue with Single Sign-On'}
        </button>
      {/if}

      {#if authConfig.oidc_enabled && authConfig.password_login_enabled}
        <div class="divider">or</div>
      {/if}

      {#if authConfig.password_login_enabled}
        <form on:submit={handleSubmit}>
          <div class="form-control">
            <label class="label" for="email">
              <span class="label-text">Email</span>
            </label>
            <input
              id="email"
              type="email"
              placeholder="admin@localhost"
              class="input input-bordered"
              bind:value={email}
              disabled={isLoading}
            />
          </div>

          <div class="form-control mt-4">
            <label class="label" for="password">
              <span class="label-text">Password</span>
            </label>
            <input
              id="password"
              type="password"
              placeholder="••••••••"
              class="input input-bordered"
              bind:value={password}
              disabled={isLoading}
            />
          </div>

          <div class="form-control mt-6">
            <button
              type="submit"
              class="btn btn-primary"
              class:loading={isLoading}
              disabled={isLoading}
            >
              {isLoading ? 'Logging in...' : 'Login'}
            </button>
          </div>
        </form>
      {:else if !authConfig.oidc_enabled}
        <div class="alert alert-warning mt-4">
          <span>No login method is enabled for this deployment.</span>
        </div>
      {/if}
    </div>
  </div>
</div>

{#if showError}
  <Toast
    message={errorMessage}
    type="error"
    onClose={() => (showError = false)}
  />
{/if}
