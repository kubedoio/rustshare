<script lang="ts">
  import { authStore } from '$lib/stores/auth';
  import { goto } from '$app/navigation';
  import Toast from '$lib/components/common/Toast.svelte';

  let email = '';
  let password = '';
  let isLoading = false;
  let errorMessage = '';
  let showError = false;

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
</script>

<svelte:head>
  <title>Login - RustShare</title>
</svelte:head>

<div class="min-h-screen flex items-center justify-center bg-base-200">
  <div class="card w-96 bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-2xl justify-center mb-4">RustShare</h2>

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
