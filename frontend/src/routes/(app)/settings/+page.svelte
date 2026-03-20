<script lang="ts">
  import { currentUser, authStore } from '$lib/stores/auth';
  import { goto } from '$app/navigation';
  import Toast from '$lib/components/common/Toast.svelte';

  let showToast = false;
  let toastMessage = '';
  let toastType: 'success' | 'error' | 'info' = 'info';

  // Theme management
  let currentTheme = 'light';

  // Get current theme from HTML element
  function getCurrentTheme() {
    if (typeof window !== 'undefined') {
      const html = document.documentElement;
      currentTheme = html.getAttribute('data-theme') || 'light';
    }
  }

  // Toggle theme
  function toggleTheme() {
    const newTheme = currentTheme === 'light' ? 'dark' : 'light';
    currentTheme = newTheme;

    if (typeof window !== 'undefined') {
      document.documentElement.setAttribute('data-theme', newTheme);
      localStorage.setItem('theme', newTheme);
    }

    showNotification(`Theme changed to ${newTheme} mode`, 'success');
  }

  // Initialize theme on mount
  if (typeof window !== 'undefined') {
    getCurrentTheme();
  }

  function showNotification(message: string, type: 'success' | 'error' | 'info') {
    toastMessage = message;
    toastType = type;
    showToast = true;
  }

  function handleLogout() {
    authStore.logout();
    goto('/login');
  }

  function formatBytes(bytes: number | undefined): string {
    if (bytes === undefined) return 'N/A';
    if (bytes === 0) return '0 Bytes';

    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
  }

  function formatDate(dateString: string | undefined): string {
    if (!dateString) return 'N/A';

    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'long',
      day: 'numeric'
    });
  }

  $: storagePercentage = $currentUser?.storage_quota && $currentUser?.storage_used
    ? Math.round(($currentUser.storage_used / $currentUser.storage_quota) * 100)
    : 0;
</script>

<svelte:head>
  <title>Settings - RustShare</title>
</svelte:head>

<div class="max-w-4xl mx-auto space-y-6">
  <h1 class="text-2xl lg:text-3xl font-bold">Settings</h1>

  <!-- Profile Information -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-xl mb-4">Profile Information</h2>

      <div class="space-y-4">
        <!-- Avatar -->
        <div class="flex items-center gap-4">
          <div class="avatar placeholder">
            <div class="bg-primary text-primary-content rounded-full w-20 h-20">
              <span class="text-3xl">{$currentUser?.display_name[0].toUpperCase()}</span>
            </div>
          </div>
          <div>
            <h3 class="text-lg font-semibold">{$currentUser?.display_name}</h3>
            <p class="text-sm text-base-content/70">{$currentUser?.email}</p>
          </div>
        </div>

        <div class="divider"></div>

        <!-- User Details -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <div class="label">
              <span class="label-text font-semibold">User ID</span>
            </div>
            <div class="text-sm text-base-content/70 font-mono">{$currentUser?.id}</div>
          </div>

          <div>
            <div class="label">
              <span class="label-text font-semibold">Account Type</span>
            </div>
            <div class="text-sm">
              {#if $currentUser?.is_admin}
                <span class="badge badge-primary">Administrator</span>
              {:else}
                <span class="badge">User</span>
              {/if}
            </div>
          </div>

          <div>
            <div class="label">
              <span class="label-text font-semibold">Member Since</span>
            </div>
            <div class="text-sm text-base-content/70">{formatDate($currentUser?.created_at)}</div>
          </div>

          <div>
            <div class="label">
              <span class="label-text font-semibold">Last Updated</span>
            </div>
            <div class="text-sm text-base-content/70">{formatDate($currentUser?.updated_at)}</div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- Storage Information -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-xl mb-4">Storage</h2>

      {#if $currentUser?.storage_quota !== undefined && $currentUser?.storage_used !== undefined}
        <div class="space-y-4">
          <!-- Storage Usage Bar -->
          <div>
            <div class="flex justify-between text-sm mb-2">
              <span>{formatBytes($currentUser.storage_used)} used</span>
              <span>{formatBytes($currentUser.storage_quota)} total</span>
            </div>
            <progress
              class="progress progress-primary w-full"
              value={$currentUser.storage_used}
              max={$currentUser.storage_quota}
            ></progress>
            <div class="text-center text-sm text-base-content/70 mt-2">
              {storagePercentage}% used
            </div>
          </div>

          <!-- Storage Details -->
          <div class="grid grid-cols-2 gap-4 text-center">
            <div class="stat bg-base-200 rounded-box p-4">
              <div class="stat-title">Used</div>
              <div class="stat-value text-lg">{formatBytes($currentUser.storage_used)}</div>
            </div>
            <div class="stat bg-base-200 rounded-box p-4">
              <div class="stat-title">Available</div>
              <div class="stat-value text-lg">
                {formatBytes(($currentUser.storage_quota || 0) - ($currentUser.storage_used || 0))}
              </div>
            </div>
          </div>
        </div>
      {:else}
        <div class="alert alert-info">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="stroke-current shrink-0 w-6 h-6">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
          </svg>
          <span>Storage information is not available. Contact your administrator to enable storage quotas.</span>
        </div>
      {/if}
    </div>
  </div>

  <!-- Appearance -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-xl mb-4">Appearance</h2>

      <div class="flex items-center justify-between">
        <div>
          <h3 class="font-semibold">Theme</h3>
          <p class="text-sm text-base-content/70">
            Switch between light and dark mode
          </p>
        </div>
        <div class="form-control">
          <label class="label cursor-pointer gap-4">
            <span class="label-text">
              {#if currentTheme === 'light'}
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z" />
                </svg>
              {:else}
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-6 h-6">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z" />
                </svg>
              {/if}
            </span>
            <input
              type="checkbox"
              class="toggle toggle-primary"
              checked={currentTheme === 'dark'}
              on:change={toggleTheme}
            />
          </label>
        </div>
      </div>
    </div>
  </div>

  <!-- Account Actions -->
  <div class="card bg-base-100 shadow-xl">
    <div class="card-body">
      <h2 class="card-title text-xl mb-4">Account</h2>

      <div class="space-y-4">
        <!-- Password Change (placeholder - backend not implemented) -->
        <div class="flex items-center justify-between p-4 bg-base-200 rounded-box">
          <div>
            <h3 class="font-semibold">Change Password</h3>
            <p class="text-sm text-base-content/70">Update your account password</p>
          </div>
          <button class="btn btn-outline btn-sm" disabled>
            Coming Soon
          </button>
        </div>

        <!-- Logout -->
        <div class="flex items-center justify-between p-4 bg-base-200 rounded-box">
          <div>
            <h3 class="font-semibold">Sign Out</h3>
            <p class="text-sm text-base-content/70">Sign out of your account</p>
          </div>
          <button
            class="btn btn-error btn-sm"
            on:click={handleLogout}
          >
            Logout
          </button>
        </div>
      </div>
    </div>
  </div>
</div>

<!-- Toast Notifications -->
{#if showToast}
  <Toast
    message={toastMessage}
    type={toastType}
    onClose={() => (showToast = false)}
  />
{/if}
