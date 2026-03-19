<script lang="ts">
  import { currentUser } from '$lib/stores/auth';

  export let onMenuClick: () => void = () => {};
  export let onHelpClick: () => void = () => {};
</script>

<header class="h-16 bg-base-100 border-b border-base-300 flex items-center justify-between px-4 lg:px-6">
  <div class="flex items-center gap-4">
    <!-- Hamburger menu (mobile only) -->
    <button
      class="btn btn-ghost btn-square lg:hidden"
      on:click={onMenuClick}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        stroke-width="1.5"
        stroke="currentColor"
        class="w-6 h-6"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5"
        />
      </svg>
    </button>

    <div class="overflow-x-auto">
      <slot name="breadcrumbs" />
    </div>
  </div>

  <div class="flex items-center gap-2 lg:gap-4">
    <!-- Help button -->
    <button
      class="btn btn-ghost btn-circle btn-sm"
      on:click={onHelpClick}
      title="Keyboard shortcuts"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        stroke-width="1.5"
        stroke="currentColor"
        class="w-5 h-5"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z"
        />
      </svg>
    </button>

    {#if $currentUser}
      <div class="dropdown dropdown-end">
        <label tabindex="0" class="btn btn-ghost btn-circle avatar placeholder">
          <div class="bg-neutral-focus text-neutral-content rounded-full w-8 lg:w-10">
            <span class="text-lg lg:text-xl">{$currentUser.display_name[0].toUpperCase()}</span>
          </div>
        </label>
        <ul
          tabindex="0"
          class="mt-3 p-2 shadow menu menu-compact dropdown-content bg-base-100 rounded-box w-52"
        >
          <li class="menu-title">
            <span class="truncate">{$currentUser.email}</span>
          </li>
          <li><a href="/settings">Settings</a></li>
        </ul>
      </div>
    {/if}
  </div>
</header>
